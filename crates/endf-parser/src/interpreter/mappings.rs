use std::collections::{HashMap, HashSet};

use crate::error::{EndfError, EndfResult};
use crate::options::{ParseOpts, ReadOpts, WriteOpts};
use crate::recipe::ast::*;
use crate::records::*;
use crate::value::{EndfKey, EndfValue};

use super::expressions::{
    eval_expr, eval_expr_known, eval_index, resolve_field, set_var_value, ExprResult, RwMode,
};
use super::state::InterpreterState;

// ---------------------------------------------------------------------------
// Helpers for converting between state::RwMode and expressions::RwMode
// ---------------------------------------------------------------------------

fn is_read(state: &InterpreterState) -> bool {
    state.rwmode == super::state::RwMode::Read
}

// ---------------------------------------------------------------------------
// Helper: convert f64 to EndfValue preserving integer type
// ---------------------------------------------------------------------------

fn f64_to_endf_value(v: f64) -> EndfValue {
    if v.is_finite() && v == (v as i64) as f64 {
        EndfValue::Int(v as i64)
    } else {
        EndfValue::Float(v)
    }
}

// ---------------------------------------------------------------------------
// Helper: evaluate variable indices, then set value (borrow-safe)
// ---------------------------------------------------------------------------

/// Evaluate index expressions and then set the variable value.
/// This two-step approach avoids simultaneous mutable+immutable borrows on state.
fn eval_and_set_var(
    name: &str,
    index_exprs: &[Expr],
    value: EndfValue,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
) -> EndfResult<()> {
    // Step 1: evaluate all indices while holding only immutable borrows.
    let evaluated_indices: Vec<Expr> = if index_exprs.is_empty() {
        Vec::new()
    } else {
        let scope_chain = state.scope_chain();
        let mut indices = Vec::with_capacity(index_exprs.len());
        for idx_expr in index_exprs {
            let idx = eval_index(
                idx_expr,
                &scope_chain,
                &state.loop_vars,
                &state.abbreviations,
                parse_opts,
            )?;
            // Wrap the evaluated index as a literal Number expression.
            indices.push(Expr::Number(idx as f64));
        }
        indices
    };

    // Step 2: now take mutable borrow and set the value.
    // Since the indices are fully evaluated literals, set_var_value won't
    // need scope_chain/loop_vars/abbreviations for index evaluation.
    let empty_scope: Vec<&EndfValue> = Vec::new();
    let empty_loop = HashMap::new();
    let empty_abbr = HashMap::new();
    let scope = state.current_scope_mut();
    set_var_value(
        name,
        &evaluated_indices,
        value,
        scope,
        &empty_scope,
        &empty_loop,
        &empty_abbr,
        parse_opts,
    )
}

// ---------------------------------------------------------------------------
// Helper: check if an Expr contains a DesiredNumber anywhere
// ---------------------------------------------------------------------------

fn contains_desired_number(expr: &Expr) -> bool {
    match expr {
        Expr::DesiredNumber(_) => true,
        Expr::Neg(inner) | Expr::Bracket(inner) => contains_desired_number(inner),
        Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) | Expr::Mod(a, b) => {
            contains_desired_number(a) || contains_desired_number(b)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Helper: check if an Expr contains an InconsistentVar anywhere
// ---------------------------------------------------------------------------

fn contains_inconsistent_var(expr: &Expr) -> bool {
    match expr {
        Expr::InconsistentVar(_) => true,
        Expr::Neg(inner) | Expr::Bracket(inner) => contains_inconsistent_var(inner),
        Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) | Expr::Mod(a, b) => {
            contains_inconsistent_var(a) || contains_inconsistent_var(b)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Helper: check if an Expr is a simple variable reference (not a computed
// expression). A simple variable is Expr::Variable or Expr::InconsistentVar.
// This is used to detect cases where a variable like AWR appears in multiple
// records and should be reassigned rather than validated.
// ---------------------------------------------------------------------------

fn is_simple_variable(expr: &Expr) -> bool {
    matches!(expr, Expr::Variable(_) | Expr::InconsistentVar(_))
}

fn get_simple_variable(expr: &Expr) -> Option<&ExtVarName> {
    match expr {
        Expr::Variable(v) | Expr::InconsistentVar(v) => Some(v),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Core: fast-path for simple field mappings (read mode)
// ---------------------------------------------------------------------------

/// Try to map fields using a fast single-pass approach.
/// Returns Ok(true) if all fields were handled, Ok(false) if the fast path
/// is not applicable (caller should fall back to the general approach).
fn try_fast_map_fields(
    exprs: &[Expr],
    field_values: &[f64],
    _field_names: &[&str],
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
) -> EndfResult<bool> {
    // First pass: classify all fields. If any are too complex, bail out.
    enum FieldAction {
        /// Set a scalar variable
        SetScalar(String),
        /// Set an indexed variable (indices are IndexSource)
        SetIndexed(String, Vec<FastIndexSrc>),
        /// Validate against a constant
        ValidateConst(f64, bool),  // (expected, is_desired)
    }

    enum FastIndexSrc {
        LoopVar(String),
        Constant(i64),
    }

    let mut actions: Vec<FieldAction> = Vec::with_capacity(exprs.len());

    for expr in exprs {
        match expr {
            Expr::Variable(v) if v.indices.is_empty() => {
                // Abbreviations are compile-time substitutions; don't store them.
                if state.abbreviations.contains_key(&v.name) {
                    return Ok(false); // Fall back to slow path for validation
                }
                actions.push(FieldAction::SetScalar(v.name.clone()));
            }
            Expr::Variable(v) => {
                // Check all indices are simple (loop var or constant)
                let mut idx_srcs = Vec::with_capacity(v.indices.len());
                for idx in &v.indices {
                    match idx {
                        Expr::Variable(iv) if iv.indices.is_empty() => {
                            // Check it's a loop var
                            if state.loop_vars.contains_key(&iv.name) {
                                idx_srcs.push(FastIndexSrc::LoopVar(iv.name.clone()));
                            } else {
                                return Ok(false); // Not a loop var, could be a datadic var
                            }
                        }
                        Expr::Number(n) => {
                            idx_srcs.push(FastIndexSrc::Constant(*n as i64));
                        }
                        _ => return Ok(false), // Complex index expression
                    }
                }
                actions.push(FieldAction::SetIndexed(v.name.clone(), idx_srcs));
            }
            Expr::InconsistentVar(v) if v.indices.is_empty() => {
                actions.push(FieldAction::SetScalar(v.name.clone()));
            }
            Expr::Number(n) => {
                actions.push(FieldAction::ValidateConst(*n, false));
            }
            Expr::DesiredNumber(n) => {
                actions.push(FieldAction::ValidateConst(*n, true));
            }
            _ => return Ok(false), // Complex expression, can't fast-path
        }
    }

    // All fields are simple. Now resolve indices (needs loop_vars),
    // then apply mutations.
    struct ResolvedAction {
        kind: ResolvedKind,
    }
    enum ResolvedKind {
        SetScalar(String, EndfValue),
        SetIndexed(String, Vec<i64>, EndfValue),
        // Validation already done, nothing to apply
    }

    let mut resolved: Vec<ResolvedAction> = Vec::new();

    for (action, &value) in actions.iter().zip(field_values.iter()) {
        match action {
            FieldAction::SetScalar(name) => {
                let val = f64_to_endf_value(value);
                resolved.push(ResolvedAction {
                    kind: ResolvedKind::SetScalar(name.clone(), val),
                });
            }
            FieldAction::SetIndexed(name, idx_srcs) => {
                let val = f64_to_endf_value(value);
                let mut indices = Vec::with_capacity(idx_srcs.len());
                for src in idx_srcs {
                    match src {
                        FastIndexSrc::LoopVar(lv) => {
                            indices.push(*state.loop_vars.get(lv.as_str()).unwrap());
                        }
                        FastIndexSrc::Constant(c) => {
                            indices.push(*c);
                        }
                    }
                }
                resolved.push(ResolvedAction {
                    kind: ResolvedKind::SetIndexed(name.clone(), indices, val),
                });
            }
            FieldAction::ValidateConst(expected, is_desired) => {
                if *expected != value {
                    if *expected == 0.0 && parse_opts.ignore_zero_mismatch {
                        // tolerated
                    } else if *is_desired && parse_opts.ignore_number_mismatch {
                        // tolerated
                    } else if state.in_lookahead() {
                        // During lookahead, mismatches are expected
                    } else {
                        return Err(EndfError::NumberMismatch {
                            expected: *expected,
                            got: value,
                            field: "fast_map".to_string(),
                        });
                    }
                }
            }
        }
    }

    // Apply all mutations
    let scope = state.current_scope_mut();
    for r in resolved {
        match r.kind {
            ResolvedKind::SetScalar(name, val) => {
                scope.insert(name.as_str(), val);
            }
            ResolvedKind::SetIndexed(name, indices, val) => {
                // Use the set_indexed_value from expressions module
                // Inline a simplified version here
                if !scope.contains_key(name.as_str()) {
                    let container = match parse_opts.array_type {
                        crate::options::ArrayType::Dict => EndfValue::new_dict(),
                        crate::options::ArrayType::List => EndfValue::new_list(),
                    };
                    scope.insert(name.as_str(), container);
                }
                let mut current = scope.get_mut(name.as_str()).unwrap();
                let n_idx = indices.len();
                for (j, &idx) in indices.iter().enumerate() {
                    let is_last = j == n_idx - 1;
                    match current {
                        EndfValue::Dict(ref mut d) => {
                            let key = EndfKey::Int(idx);
                            if is_last {
                                d.insert(key, val);
                                break;
                            }
                            if !d.contains_key(&key) {
                                let container = match parse_opts.array_type {
                                    crate::options::ArrayType::Dict => EndfValue::new_dict(),
                                    crate::options::ArrayType::List => EndfValue::new_list(),
                                };
                                d.insert(key.clone(), container);
                            }
                            current = d.get_mut(&key).unwrap();
                        }
                        EndfValue::List(ref mut l) => {
                            let uidx = idx as usize;
                            while l.len() <= uidx {
                                l.push(None);
                            }
                            if is_last {
                                l[uidx] = Some(val);
                                break;
                            }
                            if l[uidx].is_none() {
                                let container = match parse_opts.array_type {
                                    crate::options::ArrayType::Dict => EndfValue::new_dict(),
                                    crate::options::ArrayType::List => EndfValue::new_list(),
                                };
                                l[uidx] = Some(container);
                            }
                            current = l[uidx].as_mut().unwrap();
                        }
                        _ => return Ok(false), // unexpected type
                    }
                }
            }
        }
    }

    Ok(true)
}

// ---------------------------------------------------------------------------
// Core: map fields to data dictionary (read mode)
// ---------------------------------------------------------------------------

/// In read mode: evaluate each expression against the field value,
/// solve for unknowns, and store results in the data dictionary.
///
/// The Python code does up to 3 passes to handle expressions that
/// reference other fields from the same record. We replicate that here.
fn map_fields_to_datadic(
    exprs: &[Expr],
    field_values: &[f64],
    field_names: &[&str],
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
) -> EndfResult<()> {
    let n = exprs.len();
    debug_assert_eq!(n, field_values.len());
    debug_assert_eq!(n, field_names.len());

    // Fast path: if all fields are simple (scalars, constants, or indexed vars
    // with simple loop-var indices), bypass the expensive multi-pass evaluation.
    if try_fast_map_fields(exprs, field_values, field_names, state, parse_opts)? {
        return Ok(());
    }

    // Track which fields have been successfully processed.
    let mut done = vec![false; n];
    let mut last_unbound_err: Option<EndfError> = None;

    // Up to 3 passes to resolve dependencies between fields in the same record.
    // Optimization: compute scope_chain once per pass (not once per field).
    // We separate evaluation (immutable borrow) from mutation to satisfy
    // the borrow checker. All expressions are evaluated first with the
    // precomputed scope chain, then mutations are applied.
    for _pass in 0..3 {
        let mut progress = false;

        // Phase 1: Evaluate all pending expressions with a single scope_chain.
        // We store Ok results and propagate fatal errors immediately.
        // SeveralUnboundVariables is stored as None (retry on next pass).
        let mut eval_results: Vec<Option<ExprResult>> = vec![None; n];
        {
            let scope_chain = state.scope_chain();
            for i in 0..n {
                if done[i] {
                    continue;
                }
                match eval_expr(
                    &exprs[i],
                    &scope_chain,
                    &state.loop_vars,
                    &state.abbreviations,
                    parse_opts,
                ) {
                    Ok(r) => {
                        eval_results[i] = Some(r);
                    }
                    Err(EndfError::SeveralUnboundVariables) => {
                        last_unbound_err = Some(EndfError::SeveralUnboundVariables);
                        // eval_results[i] stays None — will retry on next pass.
                    }
                    Err(e) => return Err(e),
                }
            }
        } // scope_chain dropped here, releasing immutable borrows

        // Phase 2: Process results and apply mutations.
        // Track variables solved in this pass to avoid overwriting by later
        // fields that also have the same unknown (e.g., L2=NRS and N1=6*NX
        // where NX abbreviation expands to an expression linear in NRS).
        let mut solved_this_pass: HashSet<String> = HashSet::new();
        for i in 0..n {
            if done[i] {
                continue;
            }
            let r = match eval_results[i].take() {
                Some(r) => r,
                None => continue, // SeveralUnboundVariables — skip for now
            };

            if r.is_known() {
                // Expression is fully determined.
                //
                // If the expression is a simple variable reference (e.g., AWR)
                // that resolved as "known" because the variable was already set
                // by a prior record, we reassign it with the new field value.
                // This matches Python's look_up=FALSE behavior where existing
                // datadic variables are not looked up during field mapping, so
                // they enter the "solve for unknown" branch and get overwritten.
                //
                // Exception: abbreviations (e.g., NW := NEP[j]*(NA[j]+2)) are
                // compile-time substitutions — they should not be stored in the
                // output dict. When an abbreviation name appears in a field
                // position, its expanded expression evaluates as "known", and
                // we treat it as a validation (like a computed constant).
                let is_abbreviation = if let Some(var) = get_simple_variable(&exprs[i]) {
                    var.indices.is_empty() && state.abbreviations.contains_key(&var.name)
                } else {
                    false
                };
                if is_simple_variable(&exprs[i]) && !is_abbreviation {
                    let new_value = f64_to_endf_value(field_values[i]);
                    let var = get_simple_variable(&exprs[i]).unwrap();
                    let var_name = var.name.clone();
                    let var_indices = var.indices.clone();
                    eval_and_set_var(&var_name, &var_indices, new_value, state, parse_opts)?;
                } else {
                    // Expression is a literal or computed constant — validate.
                    let expected = r.value;
                    let actual = field_values[i];
                    if !values_match(expected, actual, parse_opts) {
                        let is_desired = contains_desired_number(&exprs[i]);
                        let is_inconsistent = contains_inconsistent_var(&exprs[i]);

                        if expected == 0.0 && parse_opts.ignore_zero_mismatch {
                            // Tolerated zero mismatch -- proceed.
                        } else if is_desired && parse_opts.ignore_number_mismatch {
                            // Tolerated desired-number mismatch -- proceed.
                        } else if is_inconsistent && parse_opts.ignore_varspec_mismatch {
                            // Tolerated inconsistent-var mismatch -- proceed.
                        } else if state.in_lookahead() {
                            // During lookahead, mismatches are expected.
                        } else {
                            return Err(EndfError::NumberMismatch {
                                expected,
                                got: actual,
                                field: field_names[i].to_string(),
                            });
                        }
                    }
                }
                done[i] = true;
                progress = true;
            } else {
                // Expression has one unknown -- solve for it.
                if let Some(ref var) = r.unbound_var {
                    let var_key = if var.indices.is_empty() {
                        var.name.clone()
                    } else {
                        format!("{:?}", var)
                    };
                    if solved_this_pass.contains(&var_key) {
                        // This variable was already solved by an earlier field
                        // in this pass. Skip — next pass will re-evaluate with
                        // the solved value and validate or solve remaining fields.
                        continue;
                    }
                    let solved = resolve_field(&r, field_values[i], RwMode::Read)?;
                    let var_name = var.name.clone();
                    let var_indices = var.indices.clone();
                    let new_value = f64_to_endf_value(solved);

                    eval_and_set_var(
                        &var_name,
                        &var_indices,
                        new_value,
                        state,
                        parse_opts,
                    )?;
                    solved_this_pass.insert(var_key);
                } else {
                    // No unbound variable but not fully known — shouldn't happen,
                    // but mark as done to avoid infinite loop.
                }
                done[i] = true;
                progress = true;
            }
        }

        if done.iter().all(|&d| d) {
            return Ok(());
        }
        if !progress {
            break;
        }
    }

    // If we still have unresolved fields, report the error.
    if let Some(e) = last_unbound_err {
        Err(e)
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Core: map fields from data dictionary (write mode)
// ---------------------------------------------------------------------------

/// In write mode: evaluate expressions to produce field values.
fn map_fields_from_datadic(
    exprs: &[Expr],
    state: &InterpreterState,
    parse_opts: &ParseOpts,
) -> EndfResult<Vec<f64>> {
    let scope_chain = state.scope_chain();
    let mut values = Vec::with_capacity(exprs.len());
    for expr in exprs {
        let val = eval_expr_known(
            expr,
            &scope_chain,
            &state.loop_vars,
            &state.abbreviations,
            parse_opts,
        )?;
        values.push(val);
    }
    Ok(values)
}

// ---------------------------------------------------------------------------
// Helper: value comparison with optional fuzzy matching
// ---------------------------------------------------------------------------

fn values_match(expected: f64, actual: f64, opts: &ParseOpts) -> bool {
    if opts.fuzzy_matching {
        let atol = 1e-7;
        let rtol = 1e-5;
        (expected - actual).abs() <= atol + rtol * expected.abs()
    } else {
        expected == actual
    }
}

// ---------------------------------------------------------------------------
// Helper: get the control record from the interpreter state
// ---------------------------------------------------------------------------

fn get_ctrl_from_state(state: &InterpreterState) -> EndfResult<CtrlRecord> {
    let scope_chain = state.scope_chain();
    let mat = find_int_in_scope(&scope_chain, "MAT").unwrap_or(0) as i32;
    let mf = find_int_in_scope(&scope_chain, "MF").unwrap_or(0) as i32;
    let mt = find_int_in_scope(&scope_chain, "MT").unwrap_or(0) as i32;
    Ok(CtrlRecord { mat, mf, mt })
}

fn find_int_in_scope(scope_chain: &[&EndfValue], name: &str) -> Option<i64> {
    for scope in scope_chain {
        if let Some(val) = scope.get(name) {
            // If the key exists but is not an integer (e.g., MT is a Dict
            // array like MT[k] in LRF=7 recipes), keep searching outer scopes
            // where the scalar MT=151 lives.
            if let Some(i) = val.as_int() {
                return Some(i);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Helper: get remaining lines as &str slice for multi-line reading
// ---------------------------------------------------------------------------

/// Build remaining lines for a TAB1 record: read the header to determine
/// how many body lines are needed, then allocate just enough.
fn remaining_lines_for_tab1<'a>(state: &'a InterpreterState, read_opts: &ReadOpts) -> EndfResult<Vec<&'a str>> {
    // Read header to get NR and NP
    let (cont, _ctrl) = read_cont(&state.lines[state.ofs], read_opts)?;
    let nr = cont.n1 as usize;
    let np = cont.n2 as usize;
    // 1 header + ceil(2*NR/6) + ceil(2*NP/6) body lines
    let body_lines = ((2 * nr + 5) / 6) + ((2 * np + 5) / 6);
    let total = 1 + body_lines;
    let end = (state.ofs + total).min(state.lines.len());
    Ok(state.lines[state.ofs..end].iter().map(|s| s.as_str()).collect())
}

/// Build remaining lines for a TAB2 record.
fn remaining_lines_for_tab2<'a>(state: &'a InterpreterState, read_opts: &ReadOpts) -> EndfResult<Vec<&'a str>> {
    let (cont, _ctrl) = read_cont(&state.lines[state.ofs], read_opts)?;
    let nr = cont.n1 as usize;
    let body_lines = (2 * nr + 5) / 6;
    let total = 1 + body_lines;
    let end = (state.ofs + total).min(state.lines.len());
    Ok(state.lines[state.ofs..end].iter().map(|s| s.as_str()).collect())
}

/// Build remaining lines for a LIST record.
fn remaining_lines_for_list<'a>(state: &'a InterpreterState, read_opts: &ReadOpts) -> EndfResult<Vec<&'a str>> {
    let (cont, _ctrl) = read_cont(&state.lines[state.ofs], read_opts)?;
    let npl = cont.n1 as usize;
    let body_lines = if npl == 0 { 0 } else { (npl + 5) / 6 };
    let total = 1 + body_lines;
    let end = (state.ofs + total).min(state.lines.len());
    Ok(state.lines[state.ofs..end].iter().map(|s| s.as_str()).collect())
}

// ---------------------------------------------------------------------------
// CONT / HEAD record mapping
// ---------------------------------------------------------------------------

/// Map a CONT/HEAD record's 6 fields to/from the data dictionary.
pub fn map_head_or_cont(
    fields: &[Expr; 6],
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
    read_opts: &ReadOpts,
    write_opts: &WriteOpts,
) -> EndfResult<()> {
    if is_read(state) {
        let line = state.current_line()?;
        let (rec, _ctrl) = read_cont(line, read_opts)?;
        state.advance();
        let field_values = [
            rec.c1,
            rec.c2,
            rec.l1 as f64,
            rec.l2 as f64,
            rec.n1 as f64,
            rec.n2 as f64,
        ];
        let field_names = ["C1", "C2", "L1", "L2", "N1", "N2"];
        map_fields_to_datadic(fields, &field_values, &field_names, state, parse_opts)?;
    } else {
        let field_values = map_fields_from_datadic(fields, state, parse_opts)?;
        let rec = ContRecord {
            c1: field_values[0],
            c2: field_values[1],
            l1: field_values[2] as i64,
            l2: field_values[3] as i64,
            n1: field_values[4] as i64,
            n2: field_values[5] as i64,
        };
        let ctrl = get_ctrl_from_state(state)?;
        let line = write_cont(&rec, &ctrl, write_opts);
        state.push_line(line);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// TEXT record mapping
// ---------------------------------------------------------------------------

/// Map a TEXT record.
///
/// TEXT records have one or more placeholders that map to string variables.
/// The total width is 66 characters (6 * 11). Each placeholder has an
/// optional variable name and an optional width.
pub fn map_text(
    placeholders: &[TextPlaceholder],
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
    read_opts: &ReadOpts,
    write_opts: &WriteOpts,
) -> EndfResult<()> {
    if is_read(state) {
        let line = state.current_line()?;
        let (rec, _ctrl) = read_text(line, read_opts)?;
        state.advance();

        // Split the text among placeholders based on widths.
        let total_width = read_opts.width * 6;
        let text = &rec.text;

        // Calculate widths: if a placeholder has no width, it gets the remaining space.
        let mut widths: Vec<usize> = Vec::with_capacity(placeholders.len());
        let mut assigned = 0usize;
        let mut unassigned_count = 0usize;
        for ph in placeholders {
            if let Some(w) = ph.width {
                widths.push(w);
                assigned += w;
            } else {
                widths.push(0); // placeholder
                unassigned_count += 1;
            }
        }
        if unassigned_count > 0 {
            let remaining = total_width.saturating_sub(assigned);
            let per_unassigned = remaining / unassigned_count.max(1);
            for w in widths.iter_mut() {
                if *w == 0 {
                    *w = per_unassigned;
                }
            }
        }

        // Extract substrings and store in datadic.
        let mut pos = 0;
        for (ph, &w) in placeholders.iter().zip(widths.iter()) {
            let end = (pos + w).min(text.len());
            let substr = if pos < text.len() {
                &text[pos..end]
            } else {
                ""
            };
            if let Some(ref var) = ph.var {
                let var_name = var.name.clone();
                let var_indices = var.indices.clone();
                eval_and_set_var(
                    &var_name,
                    &var_indices,
                    EndfValue::Str(substr.to_string()),
                    state,
                    parse_opts,
                )?;
            }
            pos = end;
        }
    } else {
        // Write mode: construct the text from data dictionary values.
        let total_width = write_opts.width * 6;
        let mut text = String::new();

        for ph in placeholders {
            let chunk = if let Some(ref var) = ph.var {
                let scope_chain = state.scope_chain();
                let val = super::expressions::get_var_value(
                    &var.name,
                    &var.indices,
                    &scope_chain,
                    &state.loop_vars,
                    &state.abbreviations,
                    parse_opts,
                );
                match val {
                    Some(EndfValue::Str(s)) => s,
                    Some(v) => format!("{}", v),
                    None => {
                        return Err(EndfError::VariableNotFound {
                            name: var.name.clone(),
                        })
                    }
                }
            } else {
                String::new()
            };

            let w = ph.width.unwrap_or(chunk.len());
            text.push_str(&format!("{:<width$}", chunk, width = w));
        }

        // Pad or truncate to total width.
        if text.len() < total_width {
            text.push_str(&" ".repeat(total_width - text.len()));
        }
        let text = text[..total_width].to_string();

        let rec = TextRecord { text };
        let ctrl = get_ctrl_from_state(state)?;
        let line = write_text(&rec, &ctrl, write_opts);
        state.push_line(line);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// TAB1 record mapping
// ---------------------------------------------------------------------------

/// Map a TAB1 record.
///
/// The header has 6 fields (same as CONT). Fields N1=NR and N2=NP are
/// the number of interpolation regions and points respectively.
/// After the header come the interpolation table (NBT, INT) and the
/// data table (x, y).
///
/// The first 4 header fields (C1, C2, L1, L2) are mapped normally.
/// NR is inferred from the NBT array length; NP from the x array length.
pub fn map_tab1(
    fields: &[Expr; 6],
    x_var: &ExtVarName,
    y_var: &ExtVarName,
    table_name: &Option<ExtVarName>,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
    read_opts: &ReadOpts,
    write_opts: &WriteOpts,
) -> EndfResult<()> {
    if is_read(state) {
        // Read header + body.
        let lines_ref = remaining_lines_for_tab1(state, read_opts)?;
        let (cont, body, _ctrl, new_ofs) = read_tab1(&lines_ref, 0, read_opts)?;
        let lines_consumed = new_ofs;
        state.ofs += lines_consumed;

        // Map only the first 4 header fields (C1, C2, L1, L2).
        // N1 (NR) and N2 (NP) are NOT mapped as named variables because
        // they are inferred from the table arrays (NBT.len() and X.len()).
        // This matches the Python behavior in endf_mappings.py where
        // expr_list = tab1_cont_fields.children[:-3] excludes N1/N2.
        let header_fields = &fields[..4];
        let header_values = [
            cont.c1,
            cont.c2,
            cont.l1 as f64,
            cont.l2 as f64,
        ];
        let field_names = ["C1", "C2", "L1", "L2"];
        map_fields_to_datadic(header_fields, &header_values, &field_names, state, parse_opts)?;

        // Enter table section if named.
        let section_depth = if let Some(ref tn) = table_name {
            enter_section(tn, state, parse_opts)?
        } else {
            0
        };

        // Store table data: NBT, INT arrays.
        store_int_array("NBT", &body.nbt, state, parse_opts)?;
        store_int_array("INT", &body.int, state, parse_opts)?;

        // Store x and y using the variable names from the recipe.
        store_float_array(&x_var.name, &body.x, state, parse_opts)?;
        store_float_array(&y_var.name, &body.y, state, parse_opts)?;

        // Exit table section if entered.
        if section_depth > 0 {
            exit_section(state, section_depth);
        }
    } else {
        // Write mode.

        // Enter table section if named.
        let section_depth = if let Some(ref tn) = table_name {
            enter_section_write(tn, state, parse_opts)?
        } else {
            0
        };

        // Read table data from datadic.
        let nbt = read_int_array("NBT", state, parse_opts)?;
        let int = read_int_array("INT", state, parse_opts)?;
        let x = read_float_array(&x_var.name, state, parse_opts)?;
        let y = read_float_array(&y_var.name, state, parse_opts)?;

        // Exit table section.
        if section_depth > 0 {
            exit_section(state, section_depth);
        }

        let nr = nbt.len() as i64;
        let np = x.len() as i64;

        // Evaluate only the first 4 header fields (C1, C2, L1, L2).
        // N1 (NR) and N2 (NP) are computed from the table arrays, not from
        // recipe field expressions, because the read path doesn't store them
        // and the recipe may use arbitrary variable names (like NRP, NEP)
        // that wouldn't exist in the data dictionary.
        let header_fields = &fields[..4];
        let field_values = map_fields_from_datadic(header_fields, state, parse_opts)?;
        let cont = ContRecord {
            c1: field_values[0],
            c2: field_values[1],
            l1: field_values[2] as i64,
            l2: field_values[3] as i64,
            n1: nr,
            n2: np,
        };
        let ctrl = get_ctrl_from_state(state)?;

        // Write header line.
        let header_line = write_cont(&cont, &ctrl, write_opts);
        state.push_line(header_line);

        // Write table body.
        let tab1_body = Tab1Body { nbt, int, x, y };
        let body_lines = write_tab1_body(&tab1_body, &ctrl, write_opts);
        for line in body_lines {
            state.push_line(line);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// TAB2 record mapping
// ---------------------------------------------------------------------------

/// Map a TAB2 record.
///
/// Same as TAB1 but only interpolation data (NBT, INT), no x/y.
/// The z_var is not used here directly -- it refers to the variable in
/// subsequent TAB1/LIST records.
pub fn map_tab2(
    fields: &[Expr; 6],
    _z_var: &ExtVarName,
    table_name: &Option<ExtVarName>,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
    read_opts: &ReadOpts,
    write_opts: &WriteOpts,
) -> EndfResult<()> {
    if is_read(state) {
        let lines_ref = remaining_lines_for_tab2(state, read_opts)?;
        let (cont, body, _ctrl, new_ofs) = read_tab2(&lines_ref, 0, read_opts)?;
        let lines_consumed = new_ofs;
        state.ofs += lines_consumed;

        // Map the 6 header fields. We map all 6 for consistency
        // (the Python code maps 5: C1, C2, L1, L2, N2 -- skipping N1=NR
        // because NR is inferred from NBT length). We map all 6 and let
        // the expression evaluation handle it.
        // Map only the first 4 header fields (C1, C2, L1, L2) plus N2.
        // N1 (NR) is NOT mapped as a named variable because it is inferred
        // from the NBT array length. This matches the Python behavior.
        let header_fields = [&fields[0], &fields[1], &fields[2], &fields[3], &fields[5]];
        let header_values = [
            cont.c1,
            cont.c2,
            cont.l1 as f64,
            cont.l2 as f64,
            cont.n2 as f64,
        ];
        let field_names = ["C1", "C2", "L1", "L2", "N2"];
        let header_field_refs: Vec<Expr> = header_fields.iter().map(|e| (*e).clone()).collect();
        map_fields_to_datadic(&header_field_refs, &header_values, &field_names, state, parse_opts)?;

        // Enter table section if named.
        let section_depth = if let Some(ref tn) = table_name {
            enter_section(tn, state, parse_opts)?
        } else {
            0
        };

        // Store NBT, INT.
        store_int_array("NBT", &body.nbt, state, parse_opts)?;
        store_int_array("INT", &body.int, state, parse_opts)?;

        if section_depth > 0 {
            exit_section(state, section_depth);
        }
    } else {
        // Write mode.
        let section_depth = if let Some(ref tn) = table_name {
            enter_section_write(tn, state, parse_opts)?
        } else {
            0
        };

        let nbt = read_int_array("NBT", state, parse_opts)?;
        let int = read_int_array("INT", state, parse_opts)?;

        if section_depth > 0 {
            exit_section(state, section_depth);
        }

        let nr = nbt.len() as i64;

        // Evaluate only the 5 fields that were mapped during read
        // (fields 0-3 and field 5). Field 4 (N1/NR) is skipped because
        // it may map to a recipe variable (like NRM) that was never stored.
        // NR is computed from nbt.len() instead.
        let write_fields: Vec<Expr> = [&fields[0], &fields[1], &fields[2], &fields[3], &fields[5]]
            .iter().map(|e| (*e).clone()).collect();
        let field_values = map_fields_from_datadic(&write_fields, state, parse_opts)?;
        let cont = ContRecord {
            c1: field_values[0],
            c2: field_values[1],
            l1: field_values[2] as i64,
            l2: field_values[3] as i64,
            n1: nr,
            n2: field_values[4] as i64,
        };
        let ctrl = get_ctrl_from_state(state)?;

        let header_line = write_cont(&cont, &ctrl, write_opts);
        state.push_line(header_line);

        let tab2_body = Tab2Body { nbt, int };
        let body_lines = write_tab2_body(&tab2_body, &ctrl, write_opts);
        for line in body_lines {
            state.push_line(line);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// LIST record mapping
// ---------------------------------------------------------------------------

/// Map a LIST record.
///
/// The header has 6 fields (same as CONT). Typically N1 gives the count
/// of body values. The body is a sequence of `ListItem`s that specify
/// how to map values to/from the data dictionary.
pub fn map_list(
    fields: &[Expr; 6],
    body: &[ListItem],
    list_name: &Option<ExtVarName>,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
    read_opts: &ReadOpts,
    write_opts: &WriteOpts,
) -> EndfResult<()> {
    if is_read(state) {
        // Read the full LIST record (header + body values).
        let lines_ref = remaining_lines_for_list(state, read_opts)?;
        let (cont, vals, _ctrl, new_ofs) = read_list(&lines_ref, 0, read_opts)?;
        let lines_consumed = new_ofs;
        state.ofs += lines_consumed;

        // Map the 6 header fields.
        let header_values = [
            cont.c1,
            cont.c2,
            cont.l1 as f64,
            cont.l2 as f64,
            cont.n1 as f64,
            cont.n2 as f64,
        ];
        let field_names = ["C1", "C2", "L1", "L2", "N1", "N2"];
        map_fields_to_datadic(fields, &header_values, &field_names, state, parse_opts)?;

        // Enter list section if named.
        let section_depth = if let Some(ref ln) = list_name {
            enter_section(ln, state, parse_opts)?
        } else {
            0
        };

        // Process list body items.
        let mut val_idx: usize = 0;
        process_list_items_read(body, &vals, &mut val_idx, state, parse_opts)?;

        // Verify all values were consumed.
        if val_idx < vals.len() {
            // Not a hard error in all cases, but we flag it.
            // The Python code raises UnconsumedListElementsError.
            // We'll return an error for now.
            return Err(EndfError::UnconsumedListElements {
                remaining: vals.len() - val_idx,
            });
        }

        if section_depth > 0 {
            exit_section(state, section_depth);
        }
    } else {
        // Write mode: evaluate header, then build body values.

        // Enter list section if named (for reading body values from datadic).
        let section_depth = if let Some(ref ln) = list_name {
            enter_section_write(ln, state, parse_opts)?
        } else {
            0
        };

        // Build list body values first (we may need the count for N1).
        let mut vals: Vec<f64> = Vec::new();
        process_list_items_write(body, &mut vals, state, parse_opts)?;

        if section_depth > 0 {
            exit_section(state, section_depth);
        }

        // Ensure NPL is available for header evaluation.
        {
            let scope = state.current_scope_mut();
            if !scope.contains_key("NPL") {
                scope.insert("NPL", EndfValue::Int(vals.len() as i64));
            }
        }

        let field_values = map_fields_from_datadic(fields, state, parse_opts)?;
        let cont = ContRecord {
            c1: field_values[0],
            c2: field_values[1],
            l1: field_values[2] as i64,
            l2: field_values[3] as i64,
            n1: field_values[4] as i64,
            n2: field_values[5] as i64,
        };
        let ctrl = get_ctrl_from_state(state)?;

        // Write header.
        let header_line = write_cont(&cont, &ctrl, write_opts);
        state.push_line(header_line);

        // Write body values.
        if !vals.is_empty() {
            let body_lines = write_endf_numbers(&vals, &ctrl, false, write_opts);
            for line in body_lines {
                state.push_line(line);
            }
        }
    }
    Ok(())
}

/// Process list body items in read mode, consuming values from `vals`.
fn process_list_items_read(
    items: &[ListItem],
    vals: &[f64],
    val_idx: &mut usize,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
) -> EndfResult<()> {
    for item in items {
        match item {
            ListItem::Value(expr) => {
                if *val_idx >= vals.len() {
                    return Err(EndfError::MoreListElementsExpected {
                        expected: *val_idx + 1,
                        got: vals.len(),
                    });
                }
                let fv = vals[*val_idx];
                // Map this single expression against the value.
                map_fields_to_datadic(
                    std::slice::from_ref(expr),
                    &[fv],
                    &["val"],
                    state,
                    parse_opts,
                )?;
                *val_idx += 1;
            }
            ListItem::Loop {
                body: loop_body,
                var,
                start,
                stop,
            } => {
                // Compute scope_chain once for both start and stop evaluation.
                let (start_val, stop_val) = {
                    let scope_chain = state.scope_chain();
                    let sv = eval_expr_known(
                        start,
                        &scope_chain,
                        &state.loop_vars,
                        &state.abbreviations,
                        parse_opts,
                    )? as i64;
                    let ev = eval_expr_known(
                        stop,
                        &scope_chain,
                        &state.loop_vars,
                        &state.abbreviations,
                        parse_opts,
                    )? as i64;
                    (sv, ev)
                };

                for i in start_val..=stop_val {
                    state.loop_vars.insert(var.clone(), i);
                    process_list_items_read(loop_body, vals, val_idx, state, parse_opts)?;
                }
                state.loop_vars.remove(var);
            }
            ListItem::Padding => {
                // Skip to next 6-element boundary.
                let skip = (6 - *val_idx % 6) % 6;
                *val_idx += skip;
            }
        }
    }
    Ok(())
}

/// Process list body items in write mode, appending values to `vals`.
fn process_list_items_write(
    items: &[ListItem],
    vals: &mut Vec<f64>,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
) -> EndfResult<()> {
    for item in items {
        match item {
            ListItem::Value(expr) => {
                let scope_chain = state.scope_chain();
                let val = eval_expr_known(
                    expr,
                    &scope_chain,
                    &state.loop_vars,
                    &state.abbreviations,
                    parse_opts,
                )?;
                vals.push(val);
            }
            ListItem::Loop {
                body: loop_body,
                var,
                start,
                stop,
            } => {
                // Compute scope_chain once for both start and stop evaluation.
                let (start_val, stop_val) = {
                    let scope_chain = state.scope_chain();
                    let sv = eval_expr_known(
                        start,
                        &scope_chain,
                        &state.loop_vars,
                        &state.abbreviations,
                        parse_opts,
                    )? as i64;
                    let ev = eval_expr_known(
                        stop,
                        &scope_chain,
                        &state.loop_vars,
                        &state.abbreviations,
                        parse_opts,
                    )? as i64;
                    (sv, ev)
                };

                for i in start_val..=stop_val {
                    state.loop_vars.insert(var.clone(), i);
                    process_list_items_write(loop_body, vals, state, parse_opts)?;
                }
                state.loop_vars.remove(var);
            }
            ListItem::Padding => {
                // Pad with zeros to next 6-element boundary.
                let current = vals.len();
                let skip = (6 - current % 6) % 6;
                for _ in 0..skip {
                    vals.push(0.0);
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DIR record mapping
// ---------------------------------------------------------------------------

/// Map a DIR record's 4 fields to/from the data dictionary.
///
/// DIR records have fields 1-2 blank, fields 3-6 are L1, L2, N1, N2.
pub fn map_dir(
    fields: &[Expr; 4],
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
    read_opts: &ReadOpts,
    write_opts: &WriteOpts,
) -> EndfResult<()> {
    if is_read(state) {
        let line = state.current_line()?;
        let (rec, _ctrl) = read_dir(line, read_opts)?;
        state.advance();
        let field_values = [rec.l1 as f64, rec.l2 as f64, rec.n1 as f64, rec.n2 as f64];
        let field_names = ["L1", "L2", "N1", "N2"];
        map_fields_to_datadic(fields, &field_values, &field_names, state, parse_opts)?;
    } else {
        let field_values = map_fields_from_datadic(fields, state, parse_opts)?;
        let rec = DirRecord {
            l1: field_values[0] as i64,
            l2: field_values[1] as i64,
            n1: field_values[2] as i64,
            n2: field_values[3] as i64,
        };
        let ctrl = get_ctrl_from_state(state)?;
        let line = write_dir(&rec, &ctrl, write_opts);
        state.push_line(line);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// INTG record mapping
// ---------------------------------------------------------------------------

/// Map an INTG record.
///
/// INTG records have 3 expression fields: II, JJ, KIJ.
/// The `ndigit` expression controls the digit width for KIJ packing.
pub fn map_intg(
    fields: &[Expr; 3],
    ndigit: &Expr,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
    read_opts: &ReadOpts,
    write_opts: &WriteOpts,
) -> EndfResult<()> {
    // Evaluate ndigit (must be fully known).
    let scope_chain = state.scope_chain();
    let ndigit_val = eval_expr_known(
        ndigit,
        &scope_chain,
        &state.loop_vars,
        &state.abbreviations,
        parse_opts,
    )? as usize;

    if is_read(state) {
        let line = state.current_line()?;
        let (rec, _ctrl) = read_intg(line, ndigit_val, read_opts)?;
        state.advance();

        // Map II and JJ as scalar fields.
        let scalar_values = [rec.ii as f64, rec.jj as f64];
        let scalar_names = ["II", "JJ"];
        map_fields_to_datadic(
            &fields[..2],
            &scalar_values,
            &scalar_names,
            state,
            parse_opts,
        )?;

        // Map KIJ as an array variable.
        let kij_expr = &fields[2];
        if let Expr::Variable(ref var) = kij_expr {
            let var_name = var.name.clone();
            let var_indices = var.indices.clone();
            let kij_values: Vec<Option<EndfValue>> = rec
                .kij
                .iter()
                .map(|&v| Some(EndfValue::Int(v)))
                .collect();
            eval_and_set_var(
                &var_name,
                &var_indices,
                EndfValue::List(kij_values),
                state,
                parse_opts,
            )?;
        }
    } else {
        // Evaluate II, JJ and get KIJ with a single scope_chain.
        let scope_chain = state.scope_chain();
        let ii = eval_expr_known(
            &fields[0],
            &scope_chain,
            &state.loop_vars,
            &state.abbreviations,
            parse_opts,
        )? as i64;
        let jj = eval_expr_known(
            &fields[1],
            &scope_chain,
            &state.loop_vars,
            &state.abbreviations,
            parse_opts,
        )? as i64;

        // Get KIJ array from datadic.
        let kij = if let Expr::Variable(ref var) = fields[2] {
            let val = super::expressions::get_var_value(
                &var.name,
                &var.indices,
                &scope_chain,
                &state.loop_vars,
                &state.abbreviations,
                parse_opts,
            );
            match val {
                Some(EndfValue::List(l)) => l
                    .iter()
                    .map(|v| {
                        v.as_ref()
                            .and_then(|ev| ev.as_int())
                            .unwrap_or(0)
                    })
                    .collect(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let rec = IntgRecord { ii, jj, kij };
        let ctrl = get_ctrl_from_state(state)?;
        let line = write_intg(&rec, &ctrl, ndigit_val, write_opts);
        state.push_line(line);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Section enter/exit helpers
// ---------------------------------------------------------------------------

/// Enter a named section in read mode (create it if missing).
/// Returns the number of scope keys pushed (1 for simple names, 2 for indexed).
fn enter_section(
    name: &ExtVarName,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
) -> EndfResult<usize> {
    let scope_chain = state.scope_chain();
    let keys = section_keys(name, &scope_chain, &state.loop_vars, &state.abbreviations, parse_opts)?;
    let num_keys = keys.len();

    // Ensure each level of the path exists, creating dicts as needed.
    {
        let mut current = state.current_scope_mut();
        for key in &keys {
            if !current.contains_key(key.clone()) {
                current.insert(key.clone(), EndfValue::new_dict());
            }
            current = current.get_mut(key.clone()).unwrap();
        }
    }

    state.scope_path.extend(keys);
    Ok(num_keys)
}

/// Enter a named section in write mode (it must already exist).
/// Returns the number of scope keys pushed.
fn enter_section_write(
    name: &ExtVarName,
    state: &mut InterpreterState,
    parse_opts: &ParseOpts,
) -> EndfResult<usize> {
    let scope_chain = state.scope_chain();
    let keys = section_keys(name, &scope_chain, &state.loop_vars, &state.abbreviations, parse_opts)?;
    let num_keys = keys.len();

    // Verify that the full path exists.
    {
        let mut current: &EndfValue = state.current_scope();
        for key in &keys {
            if !current.contains_key(key.clone()) {
                return Err(EndfError::MissingSection {
                    name: format!("{}", key),
                });
            }
            current = current.get(key.clone()).unwrap();
        }
    }

    state.scope_path.extend(keys);
    Ok(num_keys)
}

/// Exit a section by popping `depth` keys from the scope path.
fn exit_section(state: &mut InterpreterState, depth: usize) {
    let new_len = state.scope_path.len() - depth;
    state.scope_path.truncate(new_len);
}

/// Compute the sequence of EndfKeys for a section name.
/// Simple names produce `[Str(name)]`; indexed names like `foo[i]`
/// produce `[Str("foo"), Int(i)]`.
fn section_keys(
    name: &ExtVarName,
    scope_chain: &[&EndfValue],
    loop_vars: &HashMap<String, i64>,
    abbreviations: &HashMap<String, Expr>,
    opts: &ParseOpts,
) -> EndfResult<Vec<EndfKey>> {
    let mut keys = vec![EndfKey::Str(name.name.clone())];
    for idx_expr in &name.indices {
        let idx = eval_index(idx_expr, scope_chain, loop_vars, abbreviations, opts)?;
        keys.push(EndfKey::Int(idx));
    }
    Ok(keys)
}

// ---------------------------------------------------------------------------
// Array storage helpers
// ---------------------------------------------------------------------------

/// Store an i64 array in the current scope of the data dictionary.
fn store_int_array(
    name: &str,
    values: &[i64],
    state: &mut InterpreterState,
    _parse_opts: &ParseOpts,
) -> EndfResult<()> {
    let arr: Vec<Option<EndfValue>> = values.iter().map(|&v| Some(EndfValue::Int(v))).collect();
    let scope = state.current_scope_mut();
    scope.insert(name, EndfValue::List(arr));
    Ok(())
}

/// Store an f64 array in the current scope of the data dictionary.
fn store_float_array(
    name: &str,
    values: &[f64],
    state: &mut InterpreterState,
    _parse_opts: &ParseOpts,
) -> EndfResult<()> {
    let arr: Vec<Option<EndfValue>> = values.iter().map(|&v| Some(EndfValue::Float(v))).collect();
    let scope = state.current_scope_mut();
    scope.insert(name, EndfValue::List(arr));
    Ok(())
}

/// Read an i64 array from the current scope of the data dictionary.
fn read_int_array(
    name: &str,
    state: &InterpreterState,
    _parse_opts: &ParseOpts,
) -> EndfResult<Vec<i64>> {
    let scope = state.current_scope();
    match scope.get(name) {
        Some(EndfValue::List(l)) => Ok(l
            .iter()
            .map(|v| {
                v.as_ref()
                    .and_then(|ev| ev.as_int())
                    .unwrap_or(0)
            })
            .collect()),
        Some(EndfValue::Dict(d)) => {
            // Dict-mode arrays: collect values in key order.
            let mut entries: Vec<(i64, i64)> = d
                .iter()
                .filter_map(|(k, v)| {
                    let idx = match k {
                        EndfKey::Int(i) => *i,
                        _ => return None,
                    };
                    let val = v.as_int().unwrap_or(0);
                    Some((idx, val))
                })
                .collect();
            entries.sort_by_key(|&(idx, _)| idx);
            Ok(entries.into_iter().map(|(_, v)| v).collect())
        }
        _ => Err(EndfError::VariableNotFound {
            name: name.to_string(),
        }),
    }
}

/// Read an f64 array from the current scope of the data dictionary.
fn read_float_array(
    name: &str,
    state: &InterpreterState,
    _parse_opts: &ParseOpts,
) -> EndfResult<Vec<f64>> {
    let scope = state.current_scope();
    match scope.get(name) {
        Some(EndfValue::List(l)) => Ok(l
            .iter()
            .map(|v| {
                v.as_ref()
                    .and_then(|ev| ev.as_float())
                    .unwrap_or(0.0)
            })
            .collect()),
        Some(EndfValue::Dict(d)) => {
            let mut entries: Vec<(i64, f64)> = d
                .iter()
                .filter_map(|(k, v)| {
                    let idx = match k {
                        EndfKey::Int(i) => *i,
                        _ => return None,
                    };
                    let val = v.as_float().unwrap_or(0.0);
                    Some((idx, val))
                })
                .collect();
            entries.sort_by_key(|&(idx, _)| idx);
            Ok(entries.into_iter().map(|(_, v)| v).collect())
        }
        _ => Err(EndfError::VariableNotFound {
            name: name.to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{ParseOpts, ReadOpts, WriteOpts};
    use crate::recipe::ast::{Expr, ExtVarName};

    fn default_parse() -> ParseOpts {
        ParseOpts::default()
    }

    fn default_read() -> ReadOpts {
        ReadOpts::default()
    }

    fn default_write() -> WriteOpts {
        WriteOpts::default()
    }

    // ── CONT read ────────────────────────────────────────────────────

    #[test]
    fn test_cont_read_simple_vars() {
        // Build a CONT line with known values.
        let wopts = default_write();
        let ropts = default_read();
        let popts = default_parse();

        let rec = ContRecord {
            c1: 1.5,
            c2: 2.5,
            l1: 10,
            l2: 20,
            n1: 30,
            n2: 40,
        };
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let line = write_cont(&rec, &ctrl, &wopts);

        let mut state = InterpreterState::new_read(vec![line]);
        // Set MAT, MF, MT so ctrl spec can be satisfied.
        state.datadic.insert("MAT", EndfValue::Int(100));
        state.datadic.insert("MF", EndfValue::Int(3));
        state.datadic.insert("MT", EndfValue::Int(1));

        // Recipe: [MAT,MF,MT/ ZA, AWR, L1, L2, NRS, NP]
        let fields = [
            Expr::Variable(ExtVarName::simple("ZA")),
            Expr::Variable(ExtVarName::simple("AWR")),
            Expr::Variable(ExtVarName::simple("L1")),
            Expr::Variable(ExtVarName::simple("L2")),
            Expr::Variable(ExtVarName::simple("NRS")),
            Expr::Variable(ExtVarName::simple("NP")),
        ];

        map_head_or_cont(&fields, &mut state, &popts, &ropts, &wopts).unwrap();

        let dd = &state.datadic;
        assert_eq!(dd.get("ZA").unwrap().as_float(), Some(1.5));
        assert_eq!(dd.get("AWR").unwrap().as_float(), Some(2.5));
        assert_eq!(dd.get("L1").unwrap().as_int(), Some(10));
        assert_eq!(dd.get("L2").unwrap().as_int(), Some(20));
        assert_eq!(dd.get("NRS").unwrap().as_int(), Some(30));
        assert_eq!(dd.get("NP").unwrap().as_int(), Some(40));
        assert_eq!(state.ofs, 1);
    }

    // ── CONT write ───────────────────────────────────────────────────

    #[test]
    fn test_cont_write_simple_vars() {
        let wopts = default_write();
        let ropts = default_read();
        let popts = default_parse();

        let mut datadic = EndfValue::new_dict();
        datadic.insert("MAT", EndfValue::Int(100));
        datadic.insert("MF", EndfValue::Int(3));
        datadic.insert("MT", EndfValue::Int(1));
        datadic.insert("ZA", EndfValue::Float(26056.0));
        datadic.insert("AWR", EndfValue::Float(55.845));
        datadic.insert("L1", EndfValue::Int(0));
        datadic.insert("L2", EndfValue::Int(0));
        datadic.insert("NRS", EndfValue::Int(1));
        datadic.insert("NP", EndfValue::Int(3));

        let mut state = InterpreterState::new_write(datadic);

        let fields = [
            Expr::Variable(ExtVarName::simple("ZA")),
            Expr::Variable(ExtVarName::simple("AWR")),
            Expr::Variable(ExtVarName::simple("L1")),
            Expr::Variable(ExtVarName::simple("L2")),
            Expr::Variable(ExtVarName::simple("NRS")),
            Expr::Variable(ExtVarName::simple("NP")),
        ];

        map_head_or_cont(&fields, &mut state, &popts, &ropts, &wopts).unwrap();

        assert_eq!(state.lines.len(), 1);
        // Parse the written line back.
        let (rec, _) = read_cont(&state.lines[0], &ropts).unwrap();
        assert!((rec.c1 - 26056.0).abs() < 1e-1);
        assert!((rec.c2 - 55.845).abs() < 1e-3);
        assert_eq!(rec.n1, 1);
        assert_eq!(rec.n2, 3);
    }

    // ── CONT with literal number validation ──────────────────────────

    #[test]
    fn test_cont_read_literal_match() {
        let wopts = default_write();
        let ropts = default_read();
        let popts = ParseOpts {
            ignore_zero_mismatch: true,
            ..default_parse()
        };

        let rec = ContRecord {
            c1: 0.0,
            c2: 0.0,
            l1: 0,
            l2: 0,
            n1: 5,
            n2: 10,
        };
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let line = write_cont(&rec, &ctrl, &wopts);

        let mut state = InterpreterState::new_read(vec![line]);
        state.datadic.insert("MAT", EndfValue::Int(100));
        state.datadic.insert("MF", EndfValue::Int(3));
        state.datadic.insert("MT", EndfValue::Int(1));

        // Recipe with literal zeros for C1, C2, L1, L2.
        let fields = [
            Expr::Number(0.0),
            Expr::Number(0.0),
            Expr::Number(0.0),
            Expr::Number(0.0),
            Expr::Variable(ExtVarName::simple("NR")),
            Expr::Variable(ExtVarName::simple("NP")),
        ];

        map_head_or_cont(&fields, &mut state, &popts, &ropts, &wopts).unwrap();

        assert_eq!(state.datadic.get("NR").unwrap().as_int(), Some(5));
        assert_eq!(state.datadic.get("NP").unwrap().as_int(), Some(10));
    }

    // ── DIR read/write ───────────────────────────────────────────────

    #[test]
    fn test_dir_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let popts = default_parse();

        let rec = DirRecord {
            l1: 1,
            l2: 2,
            n1: 3,
            n2: 4,
        };
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 1,
            mt: 451,
        };
        let line = write_dir(&rec, &ctrl, &wopts);

        let mut state = InterpreterState::new_read(vec![line]);
        state.datadic.insert("MAT", EndfValue::Int(100));
        state.datadic.insert("MF", EndfValue::Int(1));
        state.datadic.insert("MT", EndfValue::Int(451));

        let fields = [
            Expr::Variable(ExtVarName::simple("L1")),
            Expr::Variable(ExtVarName::simple("L2")),
            Expr::Variable(ExtVarName::simple("N1")),
            Expr::Variable(ExtVarName::simple("N2")),
        ];

        map_dir(&fields, &mut state, &popts, &ropts, &wopts).unwrap();

        assert_eq!(state.datadic.get("L1").unwrap().as_int(), Some(1));
        assert_eq!(state.datadic.get("N2").unwrap().as_int(), Some(4));
    }

    // ── Expression with offset: 2*NR + 1 ────────────────────────────

    #[test]
    fn test_cont_read_with_expression() {
        let wopts = default_write();
        let ropts = default_read();
        let popts = default_parse();

        // If field N1 contains 7 and the recipe says "2*NR+1",
        // then NR = (7 - 1) / 2 = 3.
        let rec = ContRecord {
            c1: 0.0,
            c2: 0.0,
            l1: 0,
            l2: 0,
            n1: 7,
            n2: 0,
        };
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let line = write_cont(&rec, &ctrl, &wopts);

        let mut state = InterpreterState::new_read(vec![line]);
        state.datadic.insert("MAT", EndfValue::Int(100));
        state.datadic.insert("MF", EndfValue::Int(3));
        state.datadic.insert("MT", EndfValue::Int(1));

        // 2*NR+1 for field N1
        let fields = [
            Expr::Number(0.0),
            Expr::Number(0.0),
            Expr::Number(0.0),
            Expr::Number(0.0),
            Expr::Add(
                Box::new(Expr::Mul(
                    Box::new(Expr::Number(2.0)),
                    Box::new(Expr::Variable(ExtVarName::simple("NR"))),
                )),
                Box::new(Expr::Number(1.0)),
            ),
            Expr::Number(0.0),
        ];

        map_head_or_cont(&fields, &mut state, &popts, &ropts, &wopts).unwrap();
        assert_eq!(state.datadic.get("NR").unwrap().as_int(), Some(3));
    }
}
