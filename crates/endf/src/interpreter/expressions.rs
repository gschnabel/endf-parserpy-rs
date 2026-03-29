use std::collections::HashMap;

use crate::error::{EndfError, EndfResult};
use crate::options::{ArrayType, ParseOpts};
use crate::recipe::ast::{Expr, ExtVarName};
use crate::value::{EndfKey, EndfValue};

// ---------------------------------------------------------------------------
// ExprResult: linear form representation
// ---------------------------------------------------------------------------

/// Result of evaluating an expression.
///
/// Represents the linear form: `value + coeff * unbound_var`.
/// If fully resolved, `coeff == 0.0` and `unbound_var` is `None`.
/// If one variable is unknown: `(const, coeff, Some(var))` meaning
/// `const + coeff * var`.
#[derive(Clone, Debug)]
pub struct ExprResult {
    pub value: f64,
    pub coeff: f64,
    pub unbound_var: Option<ExtVarName>,
}

impl ExprResult {
    /// A fully-known numeric result.
    pub fn known(value: f64) -> Self {
        Self {
            value,
            coeff: 0.0,
            unbound_var: None,
        }
    }

    /// A single unknown variable with coefficient 1.
    pub fn unknown(var: ExtVarName) -> Self {
        Self {
            value: 0.0,
            coeff: 1.0,
            unbound_var: Some(var),
        }
    }

    /// Returns `true` when the value is fully determined (no unbound variable).
    pub fn is_known(&self) -> bool {
        self.coeff == 0.0
    }
}

// ---------------------------------------------------------------------------
// Read/Write mode selector
// ---------------------------------------------------------------------------

/// Direction of field resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RwMode {
    /// Reading from an ENDF file: solve for the unknown variable.
    Read,
    /// Writing to an ENDF file: compute the field value.
    Write,
}

// ---------------------------------------------------------------------------
// Main evaluation entry points
// ---------------------------------------------------------------------------

/// Evaluate an expression to a (possibly partially known) linear form.
///
/// * `expr`          -- the AST expression to evaluate
/// * `scope_chain`   -- scopes from innermost to outermost
/// * `loop_vars`     -- current loop iteration variables
/// * `abbreviations` -- abbreviation name -> defining expression
/// * `opts`          -- parse options
pub fn eval_expr(
    expr: &Expr,
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> EndfResult<ExprResult> {
    match expr {
        Expr::Number(v) => Ok(ExprResult::known(*v)),
        Expr::DesiredNumber(v) => Ok(ExprResult::known(*v)),

        Expr::Variable(var) | Expr::InconsistentVar(var) => {
            eval_variable(var, scope_chain, loop_vars, abbreviations, opts)
        }

        Expr::Neg(inner) => {
            let r = eval_expr(inner, scope_chain, loop_vars, abbreviations, opts)?;
            Ok(ExprResult {
                value: -r.value,
                coeff: -r.coeff,
                unbound_var: r.unbound_var,
            })
        }

        Expr::Add(a, b) => {
            let ra = eval_expr(a, scope_chain, loop_vars, abbreviations, opts)?;
            let rb = eval_expr(b, scope_chain, loop_vars, abbreviations, opts)?;
            combine_additive(ra, rb, 1.0)
        }

        Expr::Sub(a, b) => {
            let ra = eval_expr(a, scope_chain, loop_vars, abbreviations, opts)?;
            let rb = eval_expr(b, scope_chain, loop_vars, abbreviations, opts)?;
            combine_additive(ra, rb, -1.0)
        }

        Expr::Mul(a, b) => {
            let ra = eval_expr(a, scope_chain, loop_vars, abbreviations, opts)?;
            let rb = eval_expr(b, scope_chain, loop_vars, abbreviations, opts)?;
            combine_multiplicative(ra, rb)
        }

        Expr::Div(a, b) => {
            let ra = eval_expr(a, scope_chain, loop_vars, abbreviations, opts)?;
            let rb = eval_expr(b, scope_chain, loop_vars, abbreviations, opts)?;
            if !rb.is_known() {
                let var_name = rb
                    .unbound_var
                    .as_ref()
                    .map(|v| v.name.clone())
                    .unwrap_or_default();
                return Err(EndfError::VariableInDenominator { name: var_name });
            }
            if rb.value == 0.0 {
                return Err(EndfError::VariableInDenominator {
                    name: "division by zero".to_string(),
                });
            }
            Ok(ExprResult {
                value: int_preserving_div(ra.value, rb.value),
                coeff: ra.coeff / rb.value,
                unbound_var: ra.unbound_var,
            })
        }

        Expr::Mod(a, b) => {
            let ra = eval_expr(a, scope_chain, loop_vars, abbreviations, opts)?;
            let rb = eval_expr(b, scope_chain, loop_vars, abbreviations, opts)?;
            if !ra.is_known() || !rb.is_known() {
                return Err(EndfError::SeveralUnboundVariables);
            }
            let a_val = ra.value;
            let b_val = rb.value;
            // Integer modulo when both are integer-valued
            if is_integer(a_val) && is_integer(b_val) && b_val != 0.0 {
                Ok(ExprResult::known(
                    ((a_val as i64) % (b_val as i64)) as f64,
                ))
            } else {
                Ok(ExprResult::known(a_val % b_val))
            }
        }

        Expr::Bracket(inner) => eval_expr(inner, scope_chain, loop_vars, abbreviations, opts),
    }
}

/// Evaluate an expression and assert it is fully known.
pub fn eval_expr_known(
    expr: &Expr,
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> EndfResult<f64> {
    let r = eval_expr(expr, scope_chain, loop_vars, abbreviations, opts)?;
    if !r.is_known() {
        let var_name = r
            .unbound_var
            .as_ref()
            .map(|v| v.name.clone())
            .unwrap_or_default();
        return Err(EndfError::VariableNotFound { name: var_name });
    }
    Ok(r.value)
}

/// Evaluate an expression as an integer index (must be fully known and integer-valued).
pub fn eval_index(
    expr: &Expr,
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> EndfResult<i64> {
    let v = eval_expr_known(expr, scope_chain, loop_vars, abbreviations, opts)?;
    if !is_integer(v) {
        return Err(EndfError::InvalidInteger {
            input: format!("{}", v),
        });
    }
    Ok(v as i64)
}

// ---------------------------------------------------------------------------
// Field resolution
// ---------------------------------------------------------------------------

/// Resolve a partially-known expression against a concrete field value.
///
/// In **Read** mode: given `value + coeff * x = field_value`, solve for `x`.
/// In **Write** mode: compute `value + coeff * field_value`.
pub fn resolve_field(result: &ExprResult, field_value: f64, rwmode: RwMode) -> EndfResult<f64> {
    match rwmode {
        RwMode::Read => {
            if result.coeff == 0.0 {
                // Fully known -- nothing to solve for.
                // The caller should check consistency separately.
                Ok(result.value)
            } else {
                Ok((field_value - result.value) / result.coeff)
            }
        }
        RwMode::Write => Ok(result.value + result.coeff * field_value),
    }
}

// ---------------------------------------------------------------------------
// Variable lookup
// ---------------------------------------------------------------------------

/// Look up a variable value by name and (optional) indices.
///
/// Search order:
/// 1. `loop_vars` (for-loop iteration variables)
/// 2. `scope_chain` from innermost (index 0) to outermost
///
/// If the looked-up value is an abbreviation expression (stored in
/// `abbreviations`), it is evaluated recursively.
pub fn get_var_value(
    name: &str,
    indices: &[Expr],
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> Option<EndfValue> {
    // 1. Check loop variables first (they shadow everything).
    if indices.is_empty() {
        if let Some(&val) = loop_vars.get(name) {
            return Some(EndfValue::Int(val));
        }
    }

    // 2. Check abbreviations -- evaluate the stored expression.
    if indices.is_empty() {
        if let Some(abbr_expr) = abbreviations.get(name) {
            let result = eval_expr(abbr_expr, scope_chain, loop_vars, abbreviations, opts).ok()?;
            if result.is_known() {
                return Some(f64_to_endf_value(result.value));
            }
            return None;
        }
    }

    // 3. Walk scope chain from innermost to outermost.
    for scope in scope_chain {
        if let Some(val) = scope.get(name) {
            // If no indices, return the value directly.
            if indices.is_empty() {
                return Some(val.clone());
            }
            // Descend through indices.
            return descend_indices(val, indices, scope_chain, loop_vars, abbreviations, opts);
        }
    }

    None
}

/// Set a variable value in the data dictionary, creating intermediate
/// containers (dicts or lists depending on `opts.array_type`) as needed.
pub fn set_var_value(
    name: &str,
    indices: &[Expr],
    value: EndfValue,
    datadic: &mut EndfValue,
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> EndfResult<()> {
    if indices.is_empty() {
        datadic.insert(name, value);
        return Ok(());
    }

    // Ensure the top-level entry exists.
    if !datadic.contains_key(name) {
        let container = make_container(opts);
        datadic.insert(name, container);
    }
    let mut current = datadic.get_mut(name).unwrap();

    // Walk through indices, creating intermediate containers.
    let n = indices.len();
    for (i, idx_expr) in indices.iter().enumerate() {
        let idx = eval_index(idx_expr, scope_chain, loop_vars, abbreviations, opts)?;
        let is_last = i == n - 1;

        match current {
            EndfValue::Dict(ref mut d) => {
                let key = EndfKey::Int(idx);
                if is_last {
                    d.insert(key, value);
                    return Ok(());
                }
                if !d.contains_key(&key) {
                    d.insert(key.clone(), make_container(opts));
                }
                current = d.get_mut(&key).unwrap();
            }
            EndfValue::List(ref mut l) => {
                let uidx = idx as usize;
                // Extend list if necessary.
                while l.len() <= uidx {
                    l.push(None);
                }
                if is_last {
                    l[uidx] = Some(value);
                    return Ok(());
                }
                if l[uidx].is_none() {
                    l[uidx] = Some(make_container(opts));
                }
                current = l[uidx].as_mut().unwrap();
            }
            _ => {
                return Err(EndfError::IndexNotFound {
                    name: name.to_string(),
                    indices: vec![idx],
                });
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Evaluate a Variable/InconsistentVar node.
fn eval_variable(
    var: &ExtVarName,
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> EndfResult<ExprResult> {
    // 1. Check loop variables first (they shadow everything).
    if var.indices.is_empty() {
        if let Some(&val) = loop_vars.get(&var.name) {
            return Ok(ExprResult::known(val as f64));
        }
    }

    // 2. Check abbreviations -- recursively evaluate the defining expression.
    //    This propagates partial results (one unknown) through the abbreviation.
    if var.indices.is_empty() {
        if let Some(abbr_expr) = abbreviations.get(&var.name) {
            return eval_expr(abbr_expr, scope_chain, loop_vars, abbreviations, opts);
        }
    }

    // 3. Walk scope chain via get_var_value (skipping loop_vars and abbreviations
    //    which we already checked above).
    for scope in scope_chain {
        if let Some(val) = scope.get(var.name.as_str()) {
            if var.indices.is_empty() {
                return match val.as_float() {
                    Some(v) => Ok(ExprResult::known(v)),
                    None => Err(EndfError::VariableNotFound {
                        name: var.name.clone(),
                    }),
                };
            }
            // Has indices -- descend into nested value.
            match descend_indices(val, &var.indices, scope_chain, loop_vars, abbreviations, opts) {
                Some(nested) => {
                    return match nested.as_float() {
                        Some(v) => Ok(ExprResult::known(v)),
                        None => Err(EndfError::VariableNotFound {
                            name: var.name.clone(),
                        }),
                    };
                }
                None => return Ok(ExprResult::unknown(var.clone())),
            }
        }
    }

    // 4. Not found anywhere -- treat as unknown.
    Ok(ExprResult::unknown(var.clone()))
}

/// Combine two `ExprResult`s additively: `ra + sign * rb`.
fn combine_additive(ra: ExprResult, rb: ExprResult, sign: f64) -> EndfResult<ExprResult> {
    let both_unbound = ra.unbound_var.is_some() && rb.unbound_var.is_some();
    if both_unbound {
        return Err(EndfError::SeveralUnboundVariables);
    }

    let value = int_preserving_add(ra.value, sign * rb.value);
    let (coeff, unbound_var) = if ra.unbound_var.is_some() {
        (ra.coeff, ra.unbound_var)
    } else {
        (sign * rb.coeff, rb.unbound_var)
    };

    Ok(ExprResult {
        value,
        coeff,
        unbound_var,
    })
}

/// Combine two `ExprResult`s multiplicatively.
/// At most one side may have an unbound variable.
fn combine_multiplicative(ra: ExprResult, rb: ExprResult) -> EndfResult<ExprResult> {
    if ra.unbound_var.is_some() && rb.unbound_var.is_some() {
        return Err(EndfError::SeveralUnboundVariables);
    }

    if ra.is_known() && rb.is_known() {
        return Ok(ExprResult::known(int_preserving_mul(ra.value, rb.value)));
    }

    // One side is known, the other has an unbound variable.
    // known_val * (value + coeff * var) = known_val*value + known_val*coeff * var
    let (known_val, partial) = if ra.is_known() {
        (ra.value, rb)
    } else {
        (rb.value, ra)
    };

    Ok(ExprResult {
        value: int_preserving_mul(known_val, partial.value),
        coeff: known_val * partial.coeff,
        unbound_var: partial.unbound_var,
    })
}

/// Descend into a nested value following evaluated indices.
fn descend_indices(
    val: &EndfValue,
    indices: &[Expr],
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> Option<EndfValue> {
    let mut current = val;
    for idx_expr in indices {
        let idx = eval_index(idx_expr, scope_chain, loop_vars, abbreviations, opts).ok()?;
        match current {
            EndfValue::Dict(d) => {
                current = d.get(&EndfKey::Int(idx))?;
            }
            EndfValue::List(l) => {
                let uidx = idx as usize;
                if uidx >= l.len() {
                    return None;
                }
                current = l[uidx].as_ref()?;
            }
            _ => return None,
        }
    }
    Some(current.clone())
}

/// Create an empty container (Dict or List) based on options.
fn make_container(opts: &ParseOpts) -> EndfValue {
    match opts.array_type {
        ArrayType::Dict => EndfValue::new_dict(),
        ArrayType::List => EndfValue::new_list(),
    }
}

/// Convert an f64 to an EndfValue, preserving integer type when possible.
fn f64_to_endf_value(v: f64) -> EndfValue {
    if is_integer(v) {
        EndfValue::Int(v as i64)
    } else {
        EndfValue::Float(v)
    }
}

/// Check whether an f64 is exactly an integer value.
fn is_integer(v: f64) -> bool {
    v.is_finite() && v == (v as i64) as f64
}

/// Integer-preserving addition: if both operands and result are integer-valued,
/// return an exact integer.
fn int_preserving_add(a: f64, b: f64) -> f64 {
    if is_integer(a) && is_integer(b) {
        let result = (a as i64) + (b as i64);
        result as f64
    } else {
        a + b
    }
}

/// Integer-preserving multiplication.
fn int_preserving_mul(a: f64, b: f64) -> f64 {
    if is_integer(a) && is_integer(b) {
        let result = (a as i64) * (b as i64);
        result as f64
    } else {
        a * b
    }
}

/// Integer-preserving division: if both operands are integer-valued and the
/// division is exact, return an integer result.
fn int_preserving_div(a: f64, b: f64) -> f64 {
    if is_integer(a) && is_integer(b) && b != 0.0 {
        let ai = a as i64;
        let bi = b as i64;
        if ai % bi == 0 {
            return (ai / bi) as f64;
        }
    }
    a / b
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

    // -- basic literal evaluation --

    #[test]
    fn test_number_literal() {
        let expr = Expr::Number(42.0);
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 42.0);
    }

    #[test]
    fn test_desired_number() {
        let expr = Expr::DesiredNumber(7.5);
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 7.5);
    }

    // -- negation --

    #[test]
    fn test_negation() {
        let expr = Expr::Neg(Box::new(Expr::Number(5.0)));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, -5.0);
    }

    // -- arithmetic with known values --

    #[test]
    fn test_add_known() {
        let expr = Expr::Add(Box::new(Expr::Number(3.0)), Box::new(Expr::Number(4.0)));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 7.0);
    }

    #[test]
    fn test_sub_known() {
        let expr = Expr::Sub(Box::new(Expr::Number(10.0)), Box::new(Expr::Number(3.0)));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 7.0);
    }

    #[test]
    fn test_mul_known() {
        let expr = Expr::Mul(Box::new(Expr::Number(6.0)), Box::new(Expr::Number(7.0)));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 42.0);
    }

    #[test]
    fn test_div_known_exact() {
        let expr = Expr::Div(Box::new(Expr::Number(12.0)), Box::new(Expr::Number(4.0)));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 3.0);
    }

    #[test]
    fn test_div_known_fractional() {
        let expr = Expr::Div(Box::new(Expr::Number(7.0)), Box::new(Expr::Number(2.0)));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 3.5);
    }

    #[test]
    fn test_mod_known() {
        let expr = Expr::Mod(Box::new(Expr::Number(17.0)), Box::new(Expr::Number(5.0)));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 2.0);
    }

    // -- unbound variable arithmetic --

    #[test]
    fn test_unknown_variable() {
        let expr = Expr::Variable(ExtVarName::simple("NR"));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(!r.is_known());
        assert_eq!(r.value, 0.0);
        assert_eq!(r.coeff, 1.0);
        assert_eq!(r.unbound_var.as_ref().unwrap().name, "NR");
    }

    #[test]
    fn test_add_with_unknown() {
        // 5 + NR => value=5, coeff=1, var=NR
        let expr = Expr::Add(
            Box::new(Expr::Number(5.0)),
            Box::new(Expr::Variable(ExtVarName::simple("NR"))),
        );
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(!r.is_known());
        assert_eq!(r.value, 5.0);
        assert_eq!(r.coeff, 1.0);
        assert_eq!(r.unbound_var.as_ref().unwrap().name, "NR");
    }

    #[test]
    fn test_mul_known_times_unknown() {
        // 6 * NRS => value=0, coeff=6, var=NRS
        let expr = Expr::Mul(
            Box::new(Expr::Number(6.0)),
            Box::new(Expr::Variable(ExtVarName::simple("NRS"))),
        );
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(!r.is_known());
        assert_eq!(r.value, 0.0);
        assert_eq!(r.coeff, 6.0);
        assert_eq!(r.unbound_var.as_ref().unwrap().name, "NRS");
    }

    #[test]
    fn test_two_unknowns_error() {
        // NR + NP -- two unknowns, should error
        let expr = Expr::Add(
            Box::new(Expr::Variable(ExtVarName::simple("NR"))),
            Box::new(Expr::Variable(ExtVarName::simple("NP"))),
        );
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts());
        assert!(r.is_err());
    }

    #[test]
    fn test_div_by_unknown_error() {
        let expr = Expr::Div(
            Box::new(Expr::Number(6.0)),
            Box::new(Expr::Variable(ExtVarName::simple("NR"))),
        );
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts());
        assert!(r.is_err());
    }

    // -- resolve_field --

    #[test]
    fn test_resolve_field_read() {
        // Expression: 5 + 2*x, field_value = 11 => x = (11 - 5) / 2 = 3
        let result = ExprResult {
            value: 5.0,
            coeff: 2.0,
            unbound_var: Some(ExtVarName::simple("x")),
        };
        let x = resolve_field(&result, 11.0, RwMode::Read).unwrap();
        assert_eq!(x, 3.0);
    }

    #[test]
    fn test_resolve_field_write() {
        // Expression: 5 + 2*x, field_value (=x) = 3 => result = 5 + 2*3 = 11
        let result = ExprResult {
            value: 5.0,
            coeff: 2.0,
            unbound_var: Some(ExtVarName::simple("x")),
        };
        let v = resolve_field(&result, 3.0, RwMode::Write).unwrap();
        assert_eq!(v, 11.0);
    }

    // -- variable lookup --

    #[test]
    fn test_loop_var_lookup() {
        let mut loop_vars = HashMap::new();
        loop_vars.insert("i".to_string(), 7);

        let expr = Expr::Variable(ExtVarName::simple("i"));
        let r = eval_expr(&expr, &empty_scope(), &loop_vars, &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 7.0);
    }

    #[test]
    fn test_scope_chain_lookup() {
        let mut outer = EndfValue::new_dict();
        outer.insert("ZA", EndfValue::Float(26056.0));
        outer.insert("AWR", EndfValue::Float(55.845));

        let mut inner = EndfValue::new_dict();
        inner.insert("NR", EndfValue::Int(3));

        let scope_chain: Vec<&EndfValue> = vec![&inner, &outer];

        // NR is in inner scope
        let val = get_var_value("NR", &[], &scope_chain, &empty_loop_vars(), &empty_abbrs(), &default_opts());
        assert_eq!(val.unwrap().as_int(), Some(3));

        // ZA is in outer scope
        let val = get_var_value("ZA", &[], &scope_chain, &empty_loop_vars(), &empty_abbrs(), &default_opts());
        assert_eq!(val.unwrap().as_float(), Some(26056.0));

        // Missing variable
        let val = get_var_value("MISSING", &[], &scope_chain, &empty_loop_vars(), &empty_abbrs(), &default_opts());
        assert!(val.is_none());
    }

    #[test]
    fn test_indexed_lookup_dict() {
        let mut arr = EndfValue::new_dict();
        arr.insert(EndfKey::Int(1), EndfValue::Float(100.0));
        arr.insert(EndfKey::Int(2), EndfValue::Float(200.0));

        let mut scope = EndfValue::new_dict();
        scope.insert("E", arr);

        let scope_chain: Vec<&EndfValue> = vec![&scope];
        let indices = vec![Expr::Number(2.0)];

        let val = get_var_value("E", &indices, &scope_chain, &empty_loop_vars(), &empty_abbrs(), &default_opts());
        assert_eq!(val.unwrap().as_float(), Some(200.0));
    }

    #[test]
    fn test_set_var_simple() {
        let mut datadic = EndfValue::new_dict();
        set_var_value("NR", &[], EndfValue::Int(5), &mut datadic, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert_eq!(datadic.get("NR").unwrap().as_int(), Some(5));
    }

    #[test]
    fn test_set_var_indexed() {
        let mut datadic = EndfValue::new_dict();
        let indices = vec![Expr::Number(3.0)];
        set_var_value("E", &indices, EndfValue::Float(1.5e6), &mut datadic, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();

        let e = datadic.get("E").unwrap();
        assert!(e.is_dict());
        assert_eq!(e.get(EndfKey::Int(3)).unwrap().as_float(), Some(1.5e6));
    }

    // -- eval_index --

    #[test]
    fn test_eval_index_ok() {
        let expr = Expr::Number(5.0);
        let idx = eval_index(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert_eq!(idx, 5);
    }

    #[test]
    fn test_eval_index_non_integer() {
        let expr = Expr::Number(5.5);
        let r = eval_index(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts());
        assert!(r.is_err());
    }

    // -- abbreviation evaluation --

    #[test]
    fn test_abbreviation_lookup() {
        // Define abbreviation: HALFN := N / 2
        let mut scope = EndfValue::new_dict();
        scope.insert("N", EndfValue::Int(10));

        let scope_chain: Vec<&EndfValue> = vec![&scope];
        let mut abbrs = HashMap::new();
        abbrs.insert(
            "HALFN".to_string(),
            Expr::Div(
                Box::new(Expr::Variable(ExtVarName::simple("N"))),
                Box::new(Expr::Number(2.0)),
            ),
        );

        let val = get_var_value("HALFN", &[], &scope_chain, &empty_loop_vars(), &abbrs, &default_opts());
        assert_eq!(val.unwrap().as_int(), Some(5));
    }

    // -- integer preservation --

    #[test]
    fn test_integer_preserving_arithmetic() {
        // 6 * 7 should yield 42 (integer)
        assert_eq!(int_preserving_mul(6.0, 7.0), 42.0);
        assert!(is_integer(int_preserving_mul(6.0, 7.0)));

        // 12 / 4 = 3 (exact integer division)
        assert_eq!(int_preserving_div(12.0, 4.0), 3.0);
        assert!(is_integer(int_preserving_div(12.0, 4.0)));

        // 7 / 2 = 3.5 (not exact)
        assert_eq!(int_preserving_div(7.0, 2.0), 3.5);

        // 3 + 4 = 7
        assert_eq!(int_preserving_add(3.0, 4.0), 7.0);
        assert!(is_integer(int_preserving_add(3.0, 4.0)));
    }

    // -- bracket (pass-through) --

    #[test]
    fn test_bracket_passthrough() {
        let expr = Expr::Bracket(Box::new(Expr::Add(
            Box::new(Expr::Number(2.0)),
            Box::new(Expr::Number(3.0)),
        )));
        let r = eval_expr(&expr, &empty_scope(), &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 5.0);
    }

    // -- complex expression: 6*NRS + N --

    #[test]
    fn test_complex_expression_with_scope() {
        let mut scope = EndfValue::new_dict();
        scope.insert("NRS", EndfValue::Int(4));
        scope.insert("N", EndfValue::Int(2));

        let scope_chain: Vec<&EndfValue> = vec![&scope];

        // 6*NRS + N
        let expr = Expr::Add(
            Box::new(Expr::Mul(
                Box::new(Expr::Number(6.0)),
                Box::new(Expr::Variable(ExtVarName::simple("NRS"))),
            )),
            Box::new(Expr::Variable(ExtVarName::simple("N"))),
        );

        let r = eval_expr(&expr, &scope_chain, &empty_loop_vars(), &empty_abbrs(), &default_opts()).unwrap();
        assert!(r.is_known());
        assert_eq!(r.value, 26.0); // 6*4 + 2
    }
}
