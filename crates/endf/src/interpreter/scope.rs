use std::collections::HashMap;

use crate::error::{EndfError, EndfResult};
use crate::options::ParseOpts;
use crate::recipe::ast::{Expr, ExtVarName};
use crate::value::{EndfKey, EndfValue};

use super::expressions;

// ---------------------------------------------------------------------------
// Section scope management
// ---------------------------------------------------------------------------

/// Open a new section scope within the data dictionary.
///
/// Creates a nested dictionary at the location identified by `name` (and its
/// indices, if any). Returns the sequence of `EndfKey`s that form the path
/// from `datadic` to the newly created scope, so the caller can later
/// descend into it or build a scope chain.
///
/// When `create_missing` is `true` (write mode), missing intermediate
/// containers are created automatically. When `false` (read mode), a
/// `MissingSection` error is returned if the path does not exist.
pub fn open_section(
    datadic: &mut EndfValue,
    name: &ExtVarName,
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
    create_missing: bool,
) -> EndfResult<Vec<EndfKey>> {
    let mut path = Vec::new();

    // The section name itself is the first key.
    let name_key = EndfKey::Str(name.name.clone());
    path.push(name_key.clone());

    // Evaluate index expressions to concrete integer keys.
    let mut index_keys = Vec::new();
    for idx_expr in &name.indices {
        let idx = expressions::eval_index(idx_expr, scope_chain, loop_vars, abbreviations, opts)?;
        let key = EndfKey::Int(idx);
        index_keys.push(key);
    }

    // Ensure the top-level entry exists.
    if create_missing {
        if !datadic.contains_key(name_key.clone()) {
            datadic.insert(name_key.clone(), EndfValue::new_dict());
        }
    } else if !datadic.contains_key(name_key.clone()) {
        return Err(EndfError::MissingSection {
            name: name.name.clone(),
        });
    }

    // Descend through index keys, creating containers as needed.
    let mut current = datadic.get_mut(name_key).unwrap();
    for key in &index_keys {
        path.push(key.clone());
        match current {
            EndfValue::Dict(ref mut d) => {
                if create_missing && !d.contains_key(key) {
                    d.insert(key.clone(), EndfValue::new_dict());
                } else if !d.contains_key(key) {
                    return Err(EndfError::MissingSection {
                        name: format!("{}[{}]", name.name, key),
                    });
                }
                current = d.get_mut(key).unwrap();
            }
            EndfValue::List(ref mut l) => {
                if let EndfKey::Int(idx) = key {
                    let uidx = *idx as usize;
                    if create_missing {
                        while l.len() <= uidx {
                            l.push(None);
                        }
                        if l[uidx].is_none() {
                            l[uidx] = Some(EndfValue::new_dict());
                        }
                    } else if uidx >= l.len() || l[uidx].is_none() {
                        return Err(EndfError::MissingSection {
                            name: format!("{}[{}]", name.name, idx),
                        });
                    }
                    current = l[uidx].as_mut().unwrap();
                } else {
                    return Err(EndfError::MissingSection {
                        name: format!("{}[{}]", name.name, key),
                    });
                }
            }
            _ => {
                return Err(EndfError::MissingSection {
                    name: format!("{}[{}]", name.name, key),
                });
            }
        }
    }

    // At this point `current` should be a dict representing the section scope.
    // If it is not already a dict, make it one (only in create mode).
    if create_missing {
        if !current.is_dict() {
            // Replace the value with a fresh dict.
            *current = EndfValue::new_dict();
        }
    }

    Ok(path)
}

/// Navigate into an existing section scope within the data dictionary,
/// returning an immutable reference to it, or `None` if it doesn't exist.
pub fn get_section<'a>(
    datadic: &'a EndfValue,
    name: &ExtVarName,
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> EndfResult<Option<&'a EndfValue>> {
    let mut current = match datadic.get(name.name.as_str()) {
        Some(v) => v,
        None => return Ok(None),
    };

    for idx_expr in &name.indices {
        let idx = expressions::eval_index(idx_expr, scope_chain, loop_vars, abbreviations, opts)?;
        match current {
            EndfValue::Dict(d) => {
                let key = EndfKey::Int(idx);
                match d.get(&key) {
                    Some(v) => current = v,
                    None => return Ok(None),
                }
            }
            EndfValue::List(l) => {
                let uidx = idx as usize;
                if uidx >= l.len() {
                    return Ok(None);
                }
                match &l[uidx] {
                    Some(v) => current = v,
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        }
    }

    Ok(Some(current))
}

// ---------------------------------------------------------------------------
// Abbreviation management
// ---------------------------------------------------------------------------

/// Register an abbreviation in the current scope.
///
/// If the name is already an abbreviation, it is silently overwritten
/// (this happens when abbreviations are defined inside loops).
/// Returns an error only if the name collides with a non-abbreviation
/// variable already stored in `datadic`.
pub fn introduce_abbreviation(
    datadic: &EndfValue,
    name: &str,
    expr: &Expr,
    abbreviations: &mut HashMap<String, Expr>,
) -> EndfResult<()> {
    // Allow redefinition of existing abbreviations (e.g., inside for-loops).
    if !abbreviations.contains_key(name) && datadic.contains_key(name) {
        return Err(EndfError::AbbreviationNameCollision {
            name: name.to_string(),
        });
    }
    abbreviations.insert(name.to_string(), expr.clone());
    Ok(())
}

/// Remove a previously registered abbreviation.
pub fn remove_abbreviation(abbreviations: &mut HashMap<String, Expr>, name: &str) {
    abbreviations.remove(name);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    fn empty_scope() -> Vec<&'static EndfValue> {
        Vec::new()
    }

    fn empty_loop_vars() -> HashMap<String, i64> {
        HashMap::new()
    }

    fn empty_abbrs() -> HashMap<String, Expr> {
        HashMap::new()
    }

    fn default_opts() -> ParseOpts {
        ParseOpts::default()
    }

    // -- open_section basic --

    #[test]
    fn test_open_section_simple() {
        let mut datadic = EndfValue::new_dict();
        let name = ExtVarName::simple("subsection");

        let path = open_section(
            &mut datadic,
            &name,
            &empty_scope(),
            &empty_loop_vars(),
            &empty_abbrs(),
            &default_opts(),
            true,
        )
        .unwrap();

        assert_eq!(path.len(), 1);
        assert_eq!(path[0], EndfKey::Str("subsection".to_string()));
        assert!(datadic.get("subsection").unwrap().is_dict());
    }

    #[test]
    fn test_open_section_with_index() {
        let mut datadic = EndfValue::new_dict();
        let name = ExtVarName::with_indices("data", vec![Expr::Number(2.0)]);

        let path = open_section(
            &mut datadic,
            &name,
            &empty_scope(),
            &empty_loop_vars(),
            &empty_abbrs(),
            &default_opts(),
            true,
        )
        .unwrap();

        assert_eq!(path.len(), 2);
        assert_eq!(path[0], EndfKey::Str("data".to_string()));
        assert_eq!(path[1], EndfKey::Int(2));

        // Verify structure
        let data_entry = datadic.get("data").unwrap();
        assert!(data_entry.is_dict());
        assert!(data_entry.get(EndfKey::Int(2)).unwrap().is_dict());
    }

    #[test]
    fn test_open_section_missing_read_mode() {
        let mut datadic = EndfValue::new_dict();
        let name = ExtVarName::simple("missing");

        let result = open_section(
            &mut datadic,
            &name,
            &empty_scope(),
            &empty_loop_vars(),
            &empty_abbrs(),
            &default_opts(),
            false,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_open_section_existing_read_mode() {
        let mut datadic = EndfValue::new_dict();
        datadic.insert("existing", EndfValue::new_dict());
        let name = ExtVarName::simple("existing");

        let path = open_section(
            &mut datadic,
            &name,
            &empty_scope(),
            &empty_loop_vars(),
            &empty_abbrs(),
            &default_opts(),
            false,
        )
        .unwrap();

        assert_eq!(path.len(), 1);
        assert_eq!(path[0], EndfKey::Str("existing".to_string()));
    }

    // -- get_section --

    #[test]
    fn test_get_section_found() {
        let mut datadic = EndfValue::new_dict();
        let mut inner = EndfValue::new_dict();
        inner.insert("NR", EndfValue::Int(5));
        datadic.insert("mysect", inner);

        let name = ExtVarName::simple("mysect");
        let scope_chain: Vec<&EndfValue> = vec![&datadic];

        let result =
            get_section(&datadic, &name, &scope_chain, &empty_loop_vars(), &empty_abbrs(), &default_opts())
                .unwrap();
        assert!(result.is_some());
        let sect = result.unwrap();
        assert_eq!(sect.get("NR").unwrap().as_int(), Some(5));
    }

    #[test]
    fn test_get_section_not_found() {
        let datadic = EndfValue::new_dict();
        let name = ExtVarName::simple("nope");
        let scope_chain: Vec<&EndfValue> = vec![&datadic];

        let result =
            get_section(&datadic, &name, &scope_chain, &empty_loop_vars(), &empty_abbrs(), &default_opts())
                .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_section_indexed() {
        let mut datadic = EndfValue::new_dict();
        let mut outer = EndfValue::new_dict();
        let mut entry = EndfValue::new_dict();
        entry.insert("val", EndfValue::Int(42));
        outer.insert(EndfKey::Int(3), entry);
        datadic.insert("sect", outer);

        let name = ExtVarName::with_indices("sect", vec![Expr::Number(3.0)]);
        let scope_chain: Vec<&EndfValue> = vec![&datadic];

        let result =
            get_section(&datadic, &name, &scope_chain, &empty_loop_vars(), &empty_abbrs(), &default_opts())
                .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().get("val").unwrap().as_int(), Some(42));
    }

    // -- abbreviations --

    #[test]
    fn test_introduce_abbreviation() {
        let datadic = EndfValue::new_dict();
        let mut abbrs = HashMap::new();
        let expr = Expr::Number(42.0);

        introduce_abbreviation(&datadic, "MYABBR", &expr, &mut abbrs).unwrap();
        assert!(abbrs.contains_key("MYABBR"));
    }

    #[test]
    fn test_abbreviation_redefinition_allowed() {
        let datadic = EndfValue::new_dict();
        let mut abbrs = HashMap::new();
        let expr1 = Expr::Number(42.0);
        let expr2 = Expr::Number(99.0);

        introduce_abbreviation(&datadic, "X", &expr1, &mut abbrs).unwrap();
        // Redefining an existing abbreviation is allowed (e.g., inside loops)
        let result = introduce_abbreviation(&datadic, "X", &expr2, &mut abbrs);
        assert!(result.is_ok());
    }

    #[test]
    fn test_abbreviation_name_collision_with_variable() {
        let mut datadic = EndfValue::new_dict();
        datadic.insert("NR", EndfValue::Int(5));
        let mut abbrs = HashMap::new();
        let expr = Expr::Number(42.0);

        let result = introduce_abbreviation(&datadic, "NR", &expr, &mut abbrs);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_abbreviation() {
        let mut abbrs = HashMap::new();
        abbrs.insert("X".to_string(), Expr::Number(1.0));
        remove_abbreviation(&mut abbrs, "X");
        assert!(!abbrs.contains_key("X"));
    }

    #[test]
    fn test_remove_nonexistent_abbreviation() {
        let mut abbrs: HashMap<String, Expr> = HashMap::new();
        // Should not panic.
        remove_abbreviation(&mut abbrs, "NOPE");
    }
}
