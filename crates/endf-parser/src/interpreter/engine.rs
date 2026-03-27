use crate::error::{EndfError, EndfResult};
use crate::options::{ParseOpts, ReadOpts, WriteOpts};
use crate::recipe::ast::*;
use crate::recipe::catalogue::RecipeCatalogue;
use crate::records;
use crate::value::EndfValue;

use super::control_flow::{self, ExprEvaluator};
use super::expressions;
use super::mappings;
use super::scope;
use super::state::{InterpreterState, RwMode};

// ---------------------------------------------------------------------------
// ExprEvaluator adapter: bridges control_flow trait to expressions module
// ---------------------------------------------------------------------------

struct Evaluator<'a> {
    opts: &'a ParseOpts,
}

impl<'a> ExprEvaluator for Evaluator<'a> {
    fn eval_expr_known(
        &self,
        expr: &Expr,
        scope_chain: &[&EndfValue],
        loop_vars: &std::collections::HashMap<String, i64>,
        abbreviations: &std::collections::HashMap<String, Expr>,
    ) -> EndfResult<f64> {
        expressions::eval_expr_known(expr, scope_chain, loop_vars, abbreviations, self.opts)
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// The interpreter engine that dispatches recipe nodes to handlers.
///
/// Ties together the recipe catalogue, option structs, and all interpreter
/// sub-modules (expressions, mappings, control_flow, scope).
pub struct Engine {
    pub catalogue: RecipeCatalogue,
    pub parse_opts: ParseOpts,
    pub read_opts: ReadOpts,
    pub write_opts: WriteOpts,
}

impl Engine {
    pub fn new(
        catalogue: RecipeCatalogue,
        parse_opts: ParseOpts,
        read_opts: ReadOpts,
        write_opts: WriteOpts,
    ) -> Self {
        Self {
            catalogue,
            parse_opts,
            read_opts,
            write_opts,
        }
    }

    /// Run a sequence of recipe nodes.
    pub fn run_body(&self, body: &[RecipeNode], state: &mut InterpreterState) -> EndfResult<()> {
        for node in body {
            if !state.should_proceed(false) {
                break;
            }
            self.run_instruction(node, state)?;
        }
        Ok(())
    }

    /// Dispatch a single recipe node to its handler.
    pub fn run_instruction(
        &self,
        node: &RecipeNode,
        state: &mut InterpreterState,
    ) -> EndfResult<()> {
        match node {
            RecipeNode::HeadOrCont {
                ctrl: _,
                fields,
                subtype: _,
            } => {
                if !state.should_proceed(true) {
                    return Ok(());
                }
                mappings::map_head_or_cont(
                    fields,
                    state,
                    &self.parse_opts,
                    &self.read_opts,
                    &self.write_opts,
                )
            }
            RecipeNode::Text {
                ctrl: _,
                placeholders,
            } => {
                if !state.should_proceed(true) {
                    return Ok(());
                }
                mappings::map_text(
                    placeholders,
                    state,
                    &self.parse_opts,
                    &self.read_opts,
                    &self.write_opts,
                )
            }
            RecipeNode::Dir { ctrl: _, fields } => {
                if !state.should_proceed(true) {
                    return Ok(());
                }
                mappings::map_dir(fields, state, &self.parse_opts, &self.read_opts, &self.write_opts)
            }
            RecipeNode::Intg {
                ctrl: _,
                fields,
                ndigit,
            } => {
                if !state.should_proceed(true) {
                    return Ok(());
                }
                mappings::map_intg(
                    fields,
                    ndigit,
                    state,
                    &self.parse_opts,
                    &self.read_opts,
                    &self.write_opts,
                )
            }
            RecipeNode::Tab1 {
                ctrl: _,
                fields,
                x_var,
                y_var,
                table_name,
            } => {
                if !state.should_proceed(true) {
                    return Ok(());
                }
                mappings::map_tab1(
                    fields,
                    x_var,
                    y_var,
                    table_name,
                    state,
                    &self.parse_opts,
                    &self.read_opts,
                    &self.write_opts,
                )
            }
            RecipeNode::Tab2 {
                ctrl: _,
                fields,
                z_var,
                table_name,
            } => {
                if !state.should_proceed(true) {
                    return Ok(());
                }
                mappings::map_tab2(
                    fields,
                    z_var,
                    table_name,
                    state,
                    &self.parse_opts,
                    &self.read_opts,
                    &self.write_opts,
                )
            }
            RecipeNode::List {
                ctrl: _,
                fields,
                body,
                list_name,
            } => {
                if !state.should_proceed(true) {
                    return Ok(());
                }
                mappings::map_list(
                    fields,
                    body,
                    list_name,
                    state,
                    &self.parse_opts,
                    &self.read_opts,
                    &self.write_opts,
                )
            }
            RecipeNode::Send => {
                if !state.should_proceed(true) {
                    return Ok(());
                }
                // SEND is handled by the section splitter (read) or public API (write).
                Ok(())
            }
            RecipeNode::Stop { message } => Err(EndfError::Stop {
                message: message.clone().unwrap_or_default(),
            }),
            RecipeNode::ForLoop {
                var,
                start,
                stop,
                body,
            } => {
                let evaluator = Evaluator {
                    opts: &self.parse_opts,
                };
                control_flow::execute_for_loop(
                    var,
                    start,
                    stop,
                    body,
                    state,
                    &evaluator,
                    &mut |body_nodes, st| self.run_body(body_nodes, st),
                )
            }
            RecipeNode::RepeatLoop { var, body, until } => {
                let evaluator = Evaluator {
                    opts: &self.parse_opts,
                };
                control_flow::execute_repeat_loop(
                    var,
                    body,
                    until,
                    state,
                    &evaluator,
                    &mut |body_nodes, st| self.run_body(body_nodes, st),
                )
            }
            RecipeNode::IfClause {
                branches,
                else_body,
            } => {
                let evaluator = Evaluator {
                    opts: &self.parse_opts,
                };
                control_flow::evaluate_if_clause(
                    branches,
                    else_body,
                    state,
                    &evaluator,
                    &mut |body_nodes, st| self.run_body(body_nodes, st),
                )
            }
            RecipeNode::Section { name, body } => self.process_section(name, body, state),
            RecipeNode::Abbreviation { name, expr } => {
                // Allow redefinition of existing abbreviations (e.g., inside loops).
                // Only error if colliding with a non-abbreviation variable in READ mode.
                // In WRITE mode, abbreviations are allowed to shadow existing datadic
                // variables because the data is already stored and the abbreviation is
                // just an expression shorthand used during recipe evaluation.
                let has_abbr = state.abbreviations.contains_key(name.as_str());
                if !has_abbr && state.rwmode == RwMode::Read {
                    let has_key = state.current_scope().contains_key(name.as_str());
                    if has_key {
                        return Err(EndfError::AbbreviationNameCollision {
                            name: name.clone(),
                        });
                    }
                }
                state
                    .abbreviations
                    .insert(name.clone(), expr.as_ref().clone());
                Ok(())
            }
            RecipeNode::Comment(_) => Ok(()),
        }
    }

    fn process_section(
        &self,
        name: &ExtVarName,
        body: &[RecipeNode],
        state: &mut InterpreterState,
    ) -> EndfResult<()> {
        let create_missing = state.rwmode == RwMode::Read;

        // Pre-evaluate index expressions to avoid simultaneous mutable+immutable borrows.
        // We create a resolved name with literal Number indices.
        let resolved_name = if name.indices.is_empty() {
            name.clone()
        } else {
            let scope_chain = state.scope_chain();
            let mut resolved_indices = Vec::with_capacity(name.indices.len());
            for idx_expr in &name.indices {
                let idx = expressions::eval_index(
                    idx_expr,
                    &scope_chain,
                    &state.loop_vars,
                    &state.abbreviations,
                    &self.parse_opts,
                )?;
                resolved_indices.push(Expr::Number(idx as f64));
            }
            ExtVarName::with_indices(&name.name, resolved_indices)
        };

        let empty_scope: Vec<&EndfValue> = Vec::new();
        let empty_loop = std::collections::HashMap::new();
        let empty_abbr = std::collections::HashMap::new();
        let keys = scope::open_section(
            state.current_scope_mut(),
            &resolved_name,
            &empty_scope,
            &empty_loop,
            &empty_abbr,
            &self.parse_opts,
            create_missing,
        )?;

        let num_keys = keys.len();

        // Push scope path
        state.scope_path.extend(keys);
        let old_abbrevs = state.abbreviations.clone();

        // Run body
        let result = self.run_body(body, state);

        // Restore abbreviations and pop scope path
        state.abbreviations = old_abbrevs;
        let new_len = state.scope_path.len() - num_keys;
        state.scope_path.truncate(new_len);

        result
    }

    /// Parse a single MF/MT section from lines into a data dictionary.
    pub fn parse_section(
        &self,
        mf: i32,
        mt: i32,
        lines: Vec<String>,
    ) -> EndfResult<EndfValue> {
        let recipe = self.catalogue.get(mf, mt).ok_or_else(|| {
            EndfError::RecipeParse {
                message: format!("no recipe for MF{}/MT{}", mf, mt),
            }
        })?;

        let mut state = InterpreterState::new_read(lines);
        // Set MAT, MF, MT from first line's control record
        if !state.lines.is_empty() {
            let ctrl = records::read_ctrl(&state.lines[0], &self.read_opts)?;
            state
                .datadic
                .insert("MAT", EndfValue::Int(ctrl.mat as i64));
            state.datadic.insert("MF", EndfValue::Int(ctrl.mf as i64));
            state.datadic.insert("MT", EndfValue::Int(ctrl.mt as i64));
        }

        self.run_body(recipe, &mut state)?;
        Ok(state.datadic)
    }

    /// Write a single MF/MT section from a data dictionary to lines.
    pub fn write_section(
        &self,
        mf: i32,
        mt: i32,
        datadic: EndfValue,
    ) -> EndfResult<Vec<String>> {
        let recipe = self.catalogue.get(mf, mt).ok_or_else(|| {
            EndfError::RecipeParse {
                message: format!("no recipe for MF{}/MT{}", mf, mt),
            }
        })?;

        let mut state = InterpreterState::new_write(datadic);
        self.run_body(recipe, &mut state)?;
        Ok(state.lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::parse_recipe;

    fn make_engine_with_recipe(recipe_text: &str) -> (Engine, Vec<RecipeNode>) {
        let recipe = parse_recipe(recipe_text).expect("recipe should parse");
        let catalogue = RecipeCatalogue::endf6().expect("catalogue should load");
        let engine = Engine::new(
            catalogue,
            ParseOpts::default(),
            ReadOpts::default(),
            WriteOpts::default(),
        );
        (engine, recipe)
    }

    /// Build a standard-width ENDF line (66 data chars + 9 ctrl chars).
    fn make_line(data: &str, mat: i32, mf: i32, mt: i32) -> String {
        let padded_data = format!("{:<66}", data);
        let ctrl = format!("{:>4}{:>2}{:>3}", mat, mf, mt);
        format!("{}{}", padded_data, ctrl)
    }

    #[test]
    fn test_engine_processes_comment() {
        let (engine, recipe) = make_engine_with_recipe("# this is a comment\n");
        let mut state = InterpreterState::new_read(vec![]);
        engine.run_body(&recipe, &mut state).unwrap();
        // Should do nothing, no error
    }

    #[test]
    fn test_engine_stop_instruction() {
        let (engine, recipe) = make_engine_with_recipe("stop(\"test error\")\n");
        let mut state = InterpreterState::new_read(vec![]);
        let result = engine.run_body(&recipe, &mut state);
        assert!(result.is_err());
        match result.unwrap_err() {
            EndfError::Stop { message } => assert_eq!(message, "test error"),
            other => panic!("expected Stop error, got {:?}", other),
        }
    }
}
