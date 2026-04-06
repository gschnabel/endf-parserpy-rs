//! Match query parser and evaluator for ENDF file search.
//!
//! Parses expressions like `/1/451/ZA > 92000` and evaluates them
//! against an EndfValue tree. Supports comparison operators, logical
//! AND/OR/NOT, `exists()`, and wildcard paths.

use pest::Parser;
use pest_derive::Parser;

use crate::endf_path::{EndfPath, EndfPathElement};
use crate::value::EndfValue;

#[derive(Parser)]
#[grammar = "match_query.pest"]
struct MatchQueryParser;

/// Parse a match query string into a pest parse tree, then evaluate it.
pub fn evaluate_query(query: &str, data: &EndfValue) -> Result<bool, String> {
    let pairs = MatchQueryParser::parse(Rule::query, query)
        .map_err(|e| format!("Query parse error: {}", e))?;

    let query_pair = pairs.into_iter().next().unwrap();
    // The query rule contains logical_or as its inner rule.
    let inner = query_pair.into_inner().next().unwrap();
    eval_logical_or(inner, data)
}

/// Evaluate a query and return both the result and any path-value
/// assignments for display purposes.
pub fn evaluate_query_with_matches(
    query: &str,
    data: &EndfValue,
) -> Result<(bool, Vec<(String, String)>), String> {
    let pairs = MatchQueryParser::parse(Rule::query, query)
        .map_err(|e| format!("Query parse error: {}", e))?;

    let query_pair = pairs.into_iter().next().unwrap();
    let inner = query_pair.into_inner().next().unwrap();

    let mut assignments = Vec::new();
    let result = eval_logical_or_with_assignments(inner, data, &mut assignments)?;
    Ok((result, assignments))
}

// ---------------------------------------------------------------------------
// Recursive evaluator
// ---------------------------------------------------------------------------

use pest::iterators::Pair;

fn eval_logical_or(pair: Pair<Rule>, data: &EndfValue) -> Result<bool, String> {
    debug_assert_eq!(pair.as_rule(), Rule::logical_or);
    let mut result = false;
    for child in pair.into_inner() {
        result = result || eval_logical_and(child, data)?;
    }
    Ok(result)
}

fn eval_logical_and(pair: Pair<Rule>, data: &EndfValue) -> Result<bool, String> {
    debug_assert_eq!(pair.as_rule(), Rule::logical_and);
    let mut result = true;
    for child in pair.into_inner() {
        result = result && eval_atom(child, data)?;
    }
    Ok(result)
}

fn eval_atom(pair: Pair<Rule>, data: &EndfValue) -> Result<bool, String> {
    debug_assert_eq!(pair.as_rule(), Rule::atom);
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::atom => {
            // Negation: ! atom
            Ok(!eval_atom(inner, data)?)
        }
        Rule::endf_path => {
            // exists(path)
            let path = parse_endf_path(inner.as_str())?;
            Ok(check_exists(&path, data))
        }
        Rule::logical_or => {
            // Parenthesized: (logical_or)
            eval_logical_or(inner, data)
        }
        Rule::relation => {
            eval_relation(inner, data)
        }
        _ => Err(format!("Unexpected rule in atom: {:?}", inner.as_rule())),
    }
}

fn eval_relation(pair: Pair<Rule>, data: &EndfValue) -> Result<bool, String> {
    debug_assert_eq!(pair.as_rule(), Rule::relation);
    let mut inner = pair.into_inner();
    let left = inner.next().unwrap();
    let op = inner.next().unwrap();
    let right = inner.next().unwrap();

    let left_vals = eval_expr_to_values(left, data)?;
    let op_str = op.as_str();
    let right_vals = eval_expr_to_values(right, data)?;

    // Cartesian product: any combination that satisfies the relation.
    for lv in &left_vals {
        for rv in &right_vals {
            if compare_values(lv, rv, op_str) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Evaluate an expression to one or more f64 values.
/// For number literals: one value.
/// For paths without wildcards: one value (the field's numeric value).
/// For paths with wildcards: multiple values (one per wildcard expansion).
fn eval_expr_to_values(pair: Pair<Rule>, data: &EndfValue) -> Result<Vec<f64>, String> {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::number => {
            let n: f64 = inner.as_str().parse().map_err(|e| format!("{}", e))?;
            Ok(vec![n])
        }
        Rule::endf_path => {
            let path = parse_endf_path(inner.as_str())?;
            if path.has_wildcards() {
                let concrete = expand_wildcards(&path, data);
                let mut vals = Vec::new();
                for p in concrete {
                    if let Some(v) = p.get(data).and_then(|v| v.as_float()) {
                        vals.push(v);
                    }
                }
                Ok(vals)
            } else {
                match path.get(data).and_then(|v| v.as_float()) {
                    Some(v) => Ok(vec![v]),
                    None => Ok(vec![]), // path doesn't exist or isn't numeric
                }
            }
        }
        _ => Err(format!("Unexpected rule in expr: {:?}", inner.as_rule())),
    }
}

fn compare_values(a: &f64, b: &f64, op: &str) -> bool {
    match op {
        "==" => *a == *b,
        "!=" => *a != *b,
        "<" => *a < *b,
        ">" => *a > *b,
        "<=" => *a <= *b,
        ">=" => *a >= *b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// With-assignments variant (for display)
// ---------------------------------------------------------------------------

fn eval_logical_or_with_assignments(
    pair: Pair<Rule>,
    data: &EndfValue,
    assignments: &mut Vec<(String, String)>,
) -> Result<bool, String> {
    // For display purposes, just evaluate normally and collect
    // path-value pairs from the relation nodes.
    eval_logical_or(pair.clone(), data)?;

    // Walk the tree again to collect assignments
    collect_assignments(pair.clone(), data, assignments);
    eval_logical_or(pair, data)
}

fn collect_assignments(pair: Pair<Rule>, data: &EndfValue, assignments: &mut Vec<(String, String)>) {
    match pair.as_rule() {
        Rule::endf_path => {
            let path_str = pair.as_str();
            if let Ok(path) = parse_endf_path(path_str) {
                match path.get(data) {
                    Some(EndfValue::Int(n)) => {
                        assignments.push((path_str.to_string(), format!("{}", n)));
                    }
                    Some(EndfValue::Float(f)) | Some(EndfValue::PreservedFloat(f, _)) => {
                        assignments.push((path_str.to_string(), format!("{}", f)));
                    }
                    Some(EndfValue::Str(s)) => {
                        assignments.push((path_str.to_string(), format!("\"{}\"", s.trim())));
                    }
                    Some(EndfValue::Dict(_)) => {
                        assignments.push((path_str.to_string(), "<dict>".to_string()));
                    }
                    Some(EndfValue::List(_)) => {
                        assignments.push((path_str.to_string(), "<list>".to_string()));
                    }
                    None => {
                        assignments.push((path_str.to_string(), "<not found>".to_string()));
                    }
                }
            }
        }
        _ => {
            for child in pair.into_inner() {
                collect_assignments(child, data, assignments);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_endf_path(s: &str) -> Result<EndfPath, String> {
    EndfPath::parse(s)
}

fn check_exists(path: &EndfPath, data: &EndfValue) -> bool {
    if path.has_wildcards() {
        let concrete = expand_wildcards(path, data);
        concrete.iter().any(|p| p.exists(data))
    } else {
        path.exists(data)
    }
}

/// Expand wildcards in a path by iterating over all keys at wildcard positions.
fn expand_wildcards(path: &EndfPath, data: &EndfValue) -> Vec<EndfPath> {
    let mut results = vec![EndfPath::empty()];

    for elem in path.elements() {
        match elem {
            EndfPathElement::Wildcard => {
                let mut new_results = Vec::new();
                for partial in &results {
                    // Get the container at the current partial path.
                    if let Some(container) = partial.get(data) {
                        let keys = match container {
                            EndfValue::Dict(d) => d
                                .keys()
                                .map(|k| match k {
                                    crate::value::EndfKey::Int(n) => EndfPathElement::Int(*n),
                                    crate::value::EndfKey::Str(s) => {
                                        EndfPathElement::Str(s.clone())
                                    }
                                })
                                .collect::<Vec<_>>(),
                            EndfValue::List(l) => (0..l.len())
                                .filter(|&i| l[i].is_some())
                                .map(|i| EndfPathElement::Int(i as i64))
                                .collect(),
                            _ => vec![],
                        };
                        for key in keys {
                            let mut extended = partial.clone();
                            extended.push(key);
                            new_results.push(extended);
                        }
                    }
                }
                results = new_results;
            }
            other => {
                for partial in &mut results {
                    partial.push(other.clone());
                }
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{EndfKey, EndfValue};

    fn make_test_data() -> EndfValue {
        let mut root = EndfValue::new_dict();

        // MF=1/MT=451
        let mut mt451 = EndfValue::new_dict();
        mt451.insert("ZA", EndfValue::Float(26056.0));
        mt451.insert("AWR", EndfValue::Float(55.845));
        mt451.insert("LFI", EndfValue::Int(0));

        let mut mf1 = EndfValue::new_dict();
        mf1.insert(EndfKey::Int(451), mt451);
        root.insert(EndfKey::Int(1), mf1);

        // MF=3/MT=1, MF=3/MT=2
        let mut mt1 = EndfValue::new_dict();
        mt1.insert("NR", EndfValue::Int(1));
        mt1.insert("NP", EndfValue::Int(100));

        let mut mt2 = EndfValue::new_dict();
        mt2.insert("NR", EndfValue::Int(2));
        mt2.insert("NP", EndfValue::Int(50));

        let mut mf3 = EndfValue::new_dict();
        mf3.insert(EndfKey::Int(1), mt1);
        mf3.insert(EndfKey::Int(2), mt2);
        root.insert(EndfKey::Int(3), mf3);

        root
    }

    #[test]
    fn simple_comparison() {
        let data = make_test_data();
        assert!(evaluate_query("/1/451/ZA > 26000", &data).unwrap());
        assert!(!evaluate_query("/1/451/ZA > 30000", &data).unwrap());
        assert!(evaluate_query("/1/451/ZA == 26056", &data).unwrap());
    }

    #[test]
    fn logical_and() {
        let data = make_test_data();
        assert!(evaluate_query("/1/451/ZA > 26000 & /3/1/NR == 1", &data).unwrap());
        assert!(!evaluate_query("/1/451/ZA > 26000 & /3/1/NR == 2", &data).unwrap());
    }

    #[test]
    fn logical_or() {
        let data = make_test_data();
        assert!(evaluate_query("/1/451/ZA > 30000 | /3/1/NR == 1", &data).unwrap());
        assert!(!evaluate_query("/1/451/ZA > 30000 | /3/1/NR == 5", &data).unwrap());
    }

    #[test]
    fn negation() {
        let data = make_test_data();
        assert!(evaluate_query("!(/1/451/ZA > 30000)", &data).unwrap());
        assert!(!evaluate_query("!(/1/451/ZA > 26000)", &data).unwrap());
    }

    #[test]
    fn exists_query() {
        let data = make_test_data();
        assert!(evaluate_query("exists(/1/451/ZA)", &data).unwrap());
        assert!(!evaluate_query("exists(/8/457)", &data).unwrap());
    }

    #[test]
    fn wildcard_query() {
        let data = make_test_data();
        // /3/*/NR > 0 — should be true (both MT=1 and MT=2 have NR > 0)
        assert!(evaluate_query("/3/*/NR > 0", &data).unwrap());
        // /3/*/NR == 2 — true for MT=2
        assert!(evaluate_query("/3/*/NR == 2", &data).unwrap());
        // /3/*/NR == 3 — false for both
        assert!(!evaluate_query("/3/*/NR == 3", &data).unwrap());
    }

    #[test]
    fn nonexistent_path_is_false() {
        let data = make_test_data();
        // Path doesn't exist → no values → comparison is false
        assert!(!evaluate_query("/99/1/ZA > 0", &data).unwrap());
    }
}
