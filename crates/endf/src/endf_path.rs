//! EndfPath: slash-separated path for navigating nested EndfValue structures.
//!
//! Paths like `"1/451/xstable/E"` or `"2/151/isotope[1]/range[1]/AP"`
//! address elements in the nested dictionary returned by the ENDF parser.
//!
//! Path elements are either:
//! - **String keys** (variable names like `"ZA"`, `"xstable"`)
//! - **Integer keys** (MF/MT numbers, array indices like `1`, `451`)
//! - **Wildcards** (`*`) for iteration over all keys at a level
//!
//! Bracket notation is supported: `isotope[1]` expands to `isotope/1`.

use crate::parser::{MfMtSelector, SectionFilter};
use crate::value::{EndfKey, EndfValue};

/// A parsed path into an EndfValue tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndfPath {
    elements: Vec<EndfPathElement>,
}

/// A single element in an EndfPath.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndfPathElement {
    /// String key (variable name)
    Str(String),
    /// Integer key (MF/MT number, array index)
    Int(i64),
    /// Wildcard — matches any key at this level
    Wildcard,
}

impl EndfPath {
    /// Parse a path string like `"1/451/ZA"` or `"2/151/isotope[1]"`.
    ///
    /// Rules:
    /// - Split on `/`, ignore leading/trailing slashes and empty segments.
    /// - Digit-only segments become `Int`.
    /// - `*` becomes `Wildcard`.
    /// - Bracket notation `foo[1,2]` expands to `foo/1/2`.
    /// - Everything else is `Str`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut elements = Vec::new();
        for segment in s.split('/') {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            // Expand bracket notation: "foo[1,2]" → ["foo", "1", "2"]
            let expanded = expand_brackets(segment)?;
            for part in expanded {
                elements.push(classify_element(&part));
            }
        }
        Ok(Self { elements })
    }

    /// Create an empty path.
    pub fn empty() -> Self {
        Self {
            elements: Vec::new(),
        }
    }

    /// Number of path elements.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Whether the path is empty.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Get the element at index `i`.
    pub fn element(&self, i: usize) -> Option<&EndfPathElement> {
        self.elements.get(i)
    }

    /// Iterate over elements.
    pub fn elements(&self) -> &[EndfPathElement] {
        &self.elements
    }

    /// Create a sub-path from `start..end`.
    pub fn slice(&self, start: usize, end: usize) -> Self {
        Self {
            elements: self.elements[start..end.min(self.elements.len())].to_vec(),
        }
    }

    /// Navigate to the value at this path in `root`. Returns `None` if
    /// any intermediate element doesn't exist or isn't a container.
    pub fn get<'a>(&self, root: &'a EndfValue) -> Option<&'a EndfValue> {
        let mut current = root;
        for elem in &self.elements {
            current = get_child(current, elem)?;
        }
        Some(current)
    }

    /// Navigate to the value at this path mutably.
    pub fn get_mut<'a>(&self, root: &'a mut EndfValue) -> Option<&'a mut EndfValue> {
        let mut current = root;
        for elem in &self.elements {
            current = get_child_mut(current, elem)?;
        }
        Some(current)
    }

    /// Set the value at this path, creating intermediate Dict containers
    /// as needed. Returns `Err` if an intermediate element exists but
    /// isn't a container.
    pub fn set(&self, root: &mut EndfValue, value: EndfValue) -> Result<(), String> {
        if self.elements.is_empty() {
            return Err("cannot set at empty path".into());
        }
        let mut current = root;
        // Navigate to the parent of the target, creating intermediates.
        for elem in &self.elements[..self.elements.len() - 1] {
            let key = elem_to_key(elem).ok_or_else(|| "cannot set through wildcard".to_string())?;
            if !contains_child(current, elem) {
                insert_child(current, &key, EndfValue::new_dict());
            }
            current = get_child_mut(current, elem)
                .ok_or_else(|| format!("cannot navigate through {:?}", elem))?;
        }
        // Set the leaf.
        let last = self.elements.last().unwrap();
        let key = elem_to_key(last).ok_or_else(|| "cannot set at wildcard".to_string())?;
        insert_child(current, &key, value);
        Ok(())
    }

    /// Check whether the path exists in `root`.
    pub fn exists(&self, root: &EndfValue) -> bool {
        self.get(root).is_some()
    }

    /// Remove the element at this path. Returns the removed value, or
    /// `None` if the path doesn't exist.
    pub fn remove(&self, root: &mut EndfValue) -> Option<EndfValue> {
        if self.elements.is_empty() {
            return None;
        }
        // Navigate to the parent.
        let parent = if self.elements.len() == 1 {
            root
        } else {
            let parent_path = self.slice(0, self.elements.len() - 1);
            parent_path.get_mut(root)?
        };
        let last = self.elements.last().unwrap();
        remove_child(parent, last)
    }

    /// Derive a `SectionFilter` from the first 1-2 path elements for
    /// efficient partial parsing. Mirrors Python's `determine_include`.
    ///
    /// - 0 elements: no filter (parse everything)
    /// - 1 element (must be Int): include only that MF
    /// - 2+ elements (first two must be Int): include only that (MF, MT)
    pub fn to_include_filter(&self) -> SectionFilter {
        match (self.elements.get(0), self.elements.get(1)) {
            (Some(EndfPathElement::Int(mf)), Some(EndfPathElement::Int(mt))) => {
                SectionFilter::including([MfMtSelector::MfMt(*mf as i32, *mt as i32)])
            }
            (Some(EndfPathElement::Int(mf)), _) => {
                SectionFilter::including([MfMtSelector::Mf(*mf as i32)])
            }
            _ => SectionFilter::default(),
        }
    }

    /// Check whether this path contains any wildcard elements.
    pub fn has_wildcards(&self) -> bool {
        self.elements.iter().any(|e| matches!(e, EndfPathElement::Wildcard))
    }
}

impl std::fmt::Display for EndfPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "/")?;
        for (i, elem) in self.elements.iter().enumerate() {
            if i > 0 {
                write!(f, "/")?;
            }
            match elem {
                EndfPathElement::Str(s) => write!(f, "{}", s)?,
                EndfPathElement::Int(n) => write!(f, "{}", n)?,
                EndfPathElement::Wildcard => write!(f, "*")?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Expand bracket notation: `"foo[1,2]"` → `["foo", "1", "2"]`.
/// Plain segments return a single-element vec.
fn expand_brackets(segment: &str) -> Result<Vec<String>, String> {
    if let Some(bracket_pos) = segment.find('[') {
        if !segment.ends_with(']') {
            return Err(format!("unclosed bracket in '{}'", segment));
        }
        let name = &segment[..bracket_pos];
        let indices_str = &segment[bracket_pos + 1..segment.len() - 1];
        let mut parts = vec![name.to_string()];
        for idx in indices_str.split(',') {
            let idx = idx.trim();
            if !idx.is_empty() {
                parts.push(idx.to_string());
            }
        }
        Ok(parts)
    } else {
        Ok(vec![segment.to_string()])
    }
}

/// Classify a single path element string.
fn classify_element(s: &str) -> EndfPathElement {
    if s == "*" {
        EndfPathElement::Wildcard
    } else if let Ok(n) = s.parse::<i64>() {
        EndfPathElement::Int(n)
    } else {
        EndfPathElement::Str(s.to_string())
    }
}

/// Convert a path element to an EndfKey (None for Wildcard).
fn elem_to_key(elem: &EndfPathElement) -> Option<EndfKey> {
    match elem {
        EndfPathElement::Str(s) => Some(EndfKey::Str(s.clone())),
        EndfPathElement::Int(n) => Some(EndfKey::Int(*n)),
        EndfPathElement::Wildcard => None,
    }
}

/// Get a child value from a container by path element.
fn get_child<'a>(parent: &'a EndfValue, elem: &EndfPathElement) -> Option<&'a EndfValue> {
    match elem {
        EndfPathElement::Str(s) => parent.get(s.as_str()),
        EndfPathElement::Int(n) => {
            // Try as dict key first, then as list index.
            parent.get(EndfKey::Int(*n)).or_else(|| {
                parent
                    .as_list()
                    .and_then(|l| l.get(*n as usize))
                    .and_then(|opt| opt.as_ref())
            })
        }
        EndfPathElement::Wildcard => None, // caller must expand wildcards
    }
}

/// Get a mutable child value.
fn get_child_mut<'a>(
    parent: &'a mut EndfValue,
    elem: &EndfPathElement,
) -> Option<&'a mut EndfValue> {
    match elem {
        EndfPathElement::Str(s) => parent.get_mut(s.as_str()),
        EndfPathElement::Int(n) => {
            // Try dict first; if not a dict, try list.
            if parent.is_dict() {
                parent.get_mut(EndfKey::Int(*n))
            } else if let EndfValue::List(ref mut l) = parent {
                l.get_mut(*n as usize).and_then(|opt| opt.as_mut())
            } else {
                None
            }
        }
        EndfPathElement::Wildcard => None,
    }
}

/// Check whether a container has a child at the given element.
fn contains_child(parent: &EndfValue, elem: &EndfPathElement) -> bool {
    get_child(parent, elem).is_some()
}

/// Insert a child into a container.
fn insert_child(parent: &mut EndfValue, key: &EndfKey, value: EndfValue) {
    match parent {
        EndfValue::Dict(d) => {
            d.insert(key.clone(), value);
        }
        EndfValue::List(l) => {
            if let EndfKey::Int(idx) = key {
                let uidx = *idx as usize;
                while l.len() <= uidx {
                    l.push(None);
                }
                l[uidx] = Some(value);
            }
        }
        _ => {}
    }
}

/// Remove a child from a container, returning the removed value.
fn remove_child(parent: &mut EndfValue, elem: &EndfPathElement) -> Option<EndfValue> {
    match elem {
        EndfPathElement::Str(s) => {
            if let EndfValue::Dict(d) = parent {
                d.shift_remove(&EndfKey::Str(s.clone()))
            } else {
                None
            }
        }
        EndfPathElement::Int(n) => {
            if let EndfValue::Dict(d) = parent {
                d.shift_remove(&EndfKey::Int(*n))
            } else if let EndfValue::List(l) = parent {
                let uidx = *n as usize;
                if uidx < l.len() {
                    l[uidx].take()
                } else {
                    None
                }
            } else {
                None
            }
        }
        EndfPathElement::Wildcard => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::EndfValue;

    #[test]
    fn parse_simple() {
        let p = EndfPath::parse("1/451/ZA").unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p.elements[0], EndfPathElement::Int(1));
        assert_eq!(p.elements[1], EndfPathElement::Int(451));
        assert_eq!(p.elements[2], EndfPathElement::Str("ZA".into()));
    }

    #[test]
    fn parse_bracket_notation() {
        let p = EndfPath::parse("2/151/isotope[1]").unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(p.elements[3], EndfPathElement::Int(1));
    }

    #[test]
    fn parse_wildcard() {
        let p = EndfPath::parse("3/*/xstable").unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p.elements[1], EndfPathElement::Wildcard);
    }

    #[test]
    fn parse_leading_trailing_slash() {
        let p = EndfPath::parse("/1/451/").unwrap();
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn get_navigates_nested_dict() {
        let mut inner = EndfValue::new_dict();
        inner.insert("ZA", EndfValue::Float(26056.0));
        let mut mt = EndfValue::new_dict();
        mt.insert(EndfKey::Int(451), inner);
        let mut root = EndfValue::new_dict();
        root.insert(EndfKey::Int(1), mt);

        let path = EndfPath::parse("1/451/ZA").unwrap();
        let val = path.get(&root).unwrap();
        assert_eq!(val.as_float(), Some(26056.0));
    }

    #[test]
    fn get_missing_returns_none() {
        let root = EndfValue::new_dict();
        let path = EndfPath::parse("1/451/ZA").unwrap();
        assert!(path.get(&root).is_none());
    }

    #[test]
    fn set_creates_intermediates() {
        let mut root = EndfValue::new_dict();
        let path = EndfPath::parse("1/451/ZA").unwrap();
        path.set(&mut root, EndfValue::Float(26056.0)).unwrap();

        let val = path.get(&root).unwrap();
        assert_eq!(val.as_float(), Some(26056.0));
    }

    #[test]
    fn exists_and_remove() {
        let mut root = EndfValue::new_dict();
        let path = EndfPath::parse("1/451/ZA").unwrap();
        path.set(&mut root, EndfValue::Float(26056.0)).unwrap();
        assert!(path.exists(&root));

        let removed = path.remove(&mut root);
        assert!(removed.is_some());
        assert!(!path.exists(&root));
    }

    #[test]
    fn to_include_filter_two_levels() {
        let p = EndfPath::parse("3/1/xstable").unwrap();
        let f = p.to_include_filter();
        assert!(!f.should_skip(3, 1));
        assert!(f.should_skip(3, 2));
        assert!(f.should_skip(1, 451));
    }

    #[test]
    fn to_include_filter_one_level() {
        let p = EndfPath::parse("3").unwrap();
        let f = p.to_include_filter();
        assert!(!f.should_skip(3, 1));
        assert!(!f.should_skip(3, 2));
        assert!(f.should_skip(1, 451));
    }

    #[test]
    fn display() {
        let p = EndfPath::parse("1/451/ZA").unwrap();
        assert_eq!(format!("{}", p), "/1/451/ZA");
    }

    #[test]
    fn navigate_list() {
        let mut root = EndfValue::new_dict();
        root.insert(
            "E",
            EndfValue::List(vec![
                Some(EndfValue::Float(1.0)),
                Some(EndfValue::Float(2.0)),
                Some(EndfValue::Float(3.0)),
            ]),
        );
        let path = EndfPath::parse("E/1").unwrap();
        let val = path.get(&root).unwrap();
        assert_eq!(val.as_float(), Some(2.0));
    }
}
