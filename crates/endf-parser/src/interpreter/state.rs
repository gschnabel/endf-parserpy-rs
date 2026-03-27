use std::collections::HashMap;
use crate::error::{EndfError, EndfResult};
use crate::recipe::ast::Expr;
use crate::value::{EndfKey, EndfValue};

/// Read or Write mode.
#[derive(Clone, Debug, PartialEq)]
pub enum RwMode {
    Read,
    Write,
}

/// Complete interpreter state, cloneable for snapshot/restore.
#[derive(Clone, Debug)]
pub struct InterpreterState {
    /// The root data dictionary being populated (read) or consumed (write).
    pub datadic: EndfValue,
    /// Stack of keys representing path from root to current scope.
    /// Used to navigate into datadic for the current scope.
    pub scope_path: Vec<EndfKey>,
    /// Loop counter variables.
    pub loop_vars: HashMap<String, i64>,
    /// Currently active abbreviations (name -> expression).
    pub abbreviations: HashMap<String, Expr>,
    /// ENDF lines (input for read, output for write).
    pub lines: Vec<String>,
    /// Current line offset (read mode).
    pub ofs: usize,
    /// Read or write mode.
    pub rwmode: RwMode,
    /// Current section path for error messages.
    pub current_path: Vec<String>,
    /// Lookahead depth (Some(n) if in lookahead mode).
    pub lookahead_depth: Option<usize>,
}

impl InterpreterState {
    /// Create a new state for reading ENDF lines.
    pub fn new_read(lines: Vec<String>) -> Self {
        Self {
            datadic: EndfValue::new_dict(),
            scope_path: Vec::new(),
            loop_vars: HashMap::new(),
            abbreviations: HashMap::new(),
            lines,
            ofs: 0,
            rwmode: RwMode::Read,
            current_path: Vec::new(),
            lookahead_depth: None,
        }
    }

    /// Create a new state for writing ENDF lines from a data dictionary.
    pub fn new_write(datadic: EndfValue) -> Self {
        Self {
            datadic,
            scope_path: Vec::new(),
            loop_vars: HashMap::new(),
            abbreviations: HashMap::new(),
            lines: Vec::new(),
            ofs: 0,
            rwmode: RwMode::Write,
            current_path: Vec::new(),
            lookahead_depth: None,
        }
    }

    /// Take a snapshot of the full state for lookahead.
    pub fn snapshot(&self) -> Self {
        self.clone()
    }

    /// Restore state from a snapshot.
    pub fn restore(&mut self, snapshot: Self) {
        *self = snapshot;
    }

    /// Get the current scope (navigates datadic using scope_path).
    pub fn current_scope(&self) -> &EndfValue {
        let mut current = &self.datadic;
        for key in &self.scope_path {
            current = current
                .get(key.clone())
                .expect("scope path broken: key not found in datadic");
        }
        current
    }

    /// Get a mutable reference to the current scope.
    pub fn current_scope_mut(&mut self) -> &mut EndfValue {
        let mut current = &mut self.datadic;
        for key in &self.scope_path {
            current = current
                .get_mut(key.clone())
                .expect("scope path broken: key not found in datadic");
        }
        current
    }

    /// Build the scope chain from innermost (current) to outermost (root).
    /// Returns a Vec of references ordered innermost first.
    pub fn scope_chain(&self) -> Vec<&EndfValue> {
        let mut path_scopes = Vec::with_capacity(self.scope_path.len() + 1);
        let mut current = &self.datadic;
        path_scopes.push(current);
        for key in &self.scope_path {
            current = current
                .get(key.clone())
                .expect("scope path broken: key not found in datadic");
            path_scopes.push(current);
        }
        // Reverse so innermost is first
        path_scopes.reverse();
        path_scopes
    }

    /// Check if we're in lookahead mode.
    pub fn in_lookahead(&self) -> bool {
        self.lookahead_depth.is_some()
    }

    /// Check if we should proceed with the next action.
    /// In lookahead mode, decrements the counter for ENDF record actions.
    /// Returns false when counter reaches zero.
    pub fn should_proceed(&mut self, is_endf_action: bool) -> bool {
        if let Some(ref mut depth) = self.lookahead_depth {
            if *depth == 0 {
                return false;
            }
            if is_endf_action {
                *depth -= 1;
            }
            true
        } else {
            true
        }
    }

    /// Get the current line (read mode).
    pub fn current_line(&self) -> EndfResult<&str> {
        if self.ofs >= self.lines.len() {
            Err(EndfError::UnexpectedEndOfInput { line: self.ofs })
        } else {
            Ok(&self.lines[self.ofs])
        }
    }

    /// Advance the line offset.
    pub fn advance(&mut self) {
        self.ofs += 1;
    }

    /// Append a line (write mode).
    pub fn push_line(&mut self, line: String) {
        self.lines.push(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_read_state() {
        let lines = vec!["line1".to_string(), "line2".to_string()];
        let state = InterpreterState::new_read(lines.clone());
        assert_eq!(state.rwmode, RwMode::Read);
        assert_eq!(state.ofs, 0);
        assert_eq!(state.lines.len(), 2);
        assert!(state.datadic.is_dict());
        assert!(state.scope_path.is_empty());
        assert!(state.loop_vars.is_empty());
        assert!(!state.in_lookahead());
    }

    #[test]
    fn test_new_write_state() {
        let datadic = EndfValue::new_dict();
        let state = InterpreterState::new_write(datadic);
        assert_eq!(state.rwmode, RwMode::Write);
        assert!(state.lines.is_empty());
    }

    #[test]
    fn test_current_line_and_advance() {
        let lines = vec!["first".to_string(), "second".to_string()];
        let mut state = InterpreterState::new_read(lines);
        assert_eq!(state.current_line().unwrap(), "first");
        state.advance();
        assert_eq!(state.current_line().unwrap(), "second");
        state.advance();
        assert!(state.current_line().is_err());
    }

    #[test]
    fn test_push_line() {
        let mut state = InterpreterState::new_write(EndfValue::new_dict());
        state.push_line("output line".to_string());
        assert_eq!(state.lines.len(), 1);
        assert_eq!(state.lines[0], "output line");
    }

    #[test]
    fn test_snapshot_restore() {
        let mut state = InterpreterState::new_read(vec!["a".to_string(), "b".to_string()]);
        state.advance();
        let snap = state.snapshot();
        assert_eq!(state.ofs, 1);
        state.advance();
        assert_eq!(state.ofs, 2);
        state.restore(snap);
        assert_eq!(state.ofs, 1);
    }

    #[test]
    fn test_scope_navigation() {
        let mut state = InterpreterState::new_read(vec![]);
        // Build nested structure: datadic["sec"] = { "x" => Int(42) }
        let mut inner = EndfValue::new_dict();
        inner.insert("x", EndfValue::Int(42));
        state.datadic.insert("sec", inner);
        state.scope_path.push(EndfKey::Str("sec".to_string()));

        let scope = state.current_scope();
        assert_eq!(scope.get("x").unwrap().as_int(), Some(42));

        let scope_mut = state.current_scope_mut();
        scope_mut.insert("y", EndfValue::Int(99));

        assert_eq!(
            state.datadic.get("sec").unwrap().get("y").unwrap().as_int(),
            Some(99)
        );
    }

    #[test]
    fn test_scope_chain_order() {
        let mut state = InterpreterState::new_read(vec![]);
        let mut inner = EndfValue::new_dict();
        inner.insert("val", EndfValue::Int(10));
        state.datadic.insert("outer", inner);
        state.scope_path.push(EndfKey::Str("outer".to_string()));

        let chain = state.scope_chain();
        // Innermost first
        assert_eq!(chain.len(), 2);
        // First element is the inner scope (has "val")
        assert!(chain[0].get("val").is_some());
        // Second element is root (has "outer")
        assert!(chain[1].get("outer").is_some());
    }

    #[test]
    fn test_should_proceed_no_lookahead() {
        let mut state = InterpreterState::new_read(vec![]);
        assert!(state.should_proceed(true));
        assert!(state.should_proceed(false));
    }

    #[test]
    fn test_should_proceed_with_lookahead() {
        let mut state = InterpreterState::new_read(vec![]);
        state.lookahead_depth = Some(2);

        // Non-ENDF actions don't decrement
        assert!(state.should_proceed(false));
        assert_eq!(state.lookahead_depth, Some(2));

        // ENDF actions decrement
        assert!(state.should_proceed(true));
        assert_eq!(state.lookahead_depth, Some(1));

        assert!(state.should_proceed(true));
        assert_eq!(state.lookahead_depth, Some(0));

        // Now at zero, should not proceed
        assert!(!state.should_proceed(true));
        assert!(!state.should_proceed(false));
    }
}
