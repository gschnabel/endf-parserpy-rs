//! Hot-path specialization for common for-loop patterns.
//!
//! The dominant pattern in large ENDF files is a for-loop containing a single
//! TAB1 or LIST record. For Fe-56, MF6 (55% of data) and MF33 (31%) consist
//! almost entirely of these patterns. The interpreter normally dispatches each
//! record through: `run_body -> match RecipeNode -> map_tab1 -> map_fields_to_datadic
//! -> eval_expr * 6 -> scope_chain() -> ...`. For simple records where all fields
//! are plain variables or constants, this overhead is unnecessary.
//!
//! This module detects qualifying for-loop bodies at the AST level and provides
//! specialized readers that bypass expression evaluation entirely.

use std::collections::HashSet;
use crate::error::{EndfError, EndfResult};
use crate::options::{ParseOpts, ReadOpts};
use crate::recipe::ast::*;
use crate::records;
use crate::value::{EndfKey, EndfValue};

use super::state::InterpreterState;

// ---------------------------------------------------------------------------
// Field mapping classification
// ---------------------------------------------------------------------------

/// How a single header field maps to the data dictionary.
#[derive(Clone, Debug)]
pub enum FieldMapping {
    /// Store field value into this scalar variable name.
    Variable(String),
    /// Store field value into an indexed variable: name[indices].
    IndexedVariable {
        name: String,
        indices: Vec<IndexSource>,
    },
    /// Expect this constant value (validate but don't store).
    Constant(f64),
    /// Expect this desired-number constant (validate with ignore_number_mismatch).
    DesiredConstant(f64),
}

/// A section name that may have index sources (for indexed sections like `angtable[i]`).
#[derive(Clone, Debug)]
pub struct FastSectionName {
    pub name: String,
    pub indices: Vec<IndexSource>,
}

/// Describes a for-loop body that qualifies for fast-path processing.
#[derive(Clone, Debug)]
pub enum FastPattern {
    /// For-loop body is a single TAB1 with simple field mappings.
    Tab1Loop {
        /// Variable names for C1, C2, L1, L2 (None = not mapped / padding)
        field_maps: [Option<FieldMapping>; 4],
        x_var: String,
        y_var: String,
        table_name: Option<FastSectionName>,
    },
    /// For-loop body is a single LIST with simple field mappings.
    ListLoop {
        /// Variable names for all 6 header fields
        field_maps: [Option<FieldMapping>; 6],
        /// Flattened body items: each is a simple indexed variable
        body_vars: Vec<ListBodyMapping>,
        list_name: Option<FastSectionName>,
    },
}

/// How a LIST body item maps to the data dictionary.
#[derive(Clone, Debug)]
pub enum ListBodyMapping {
    /// A simple indexed variable: name[loop_var_expr]
    /// We store the variable name and the index expression.
    IndexedVar {
        name: String,
        /// Pre-analyzed index: either a single loop variable reference
        /// or a two-variable index [outer_loop, inner_loop_var].
        indices: Vec<IndexSource>,
    },
    /// A constant that should just be skipped/validated.
    Constant(f64),
}

/// Source of an index value for fast-path variable indexing.
#[derive(Clone, Debug)]
pub enum IndexSource {
    /// Index comes from a loop variable.
    LoopVar(String),
    /// Index is a constant offset.
    Constant(i64),
}

// ---------------------------------------------------------------------------
// Pattern detection
// ---------------------------------------------------------------------------

/// Classify a single field expression into a FieldMapping.
/// Returns None if the expression is too complex for fast-path.
fn classify_field(expr: &Expr) -> Option<Option<FieldMapping>> {
    match expr {
        Expr::Variable(v) if v.indices.is_empty() => {
            Some(Some(FieldMapping::Variable(v.name.clone())))
        }
        Expr::Variable(v) => {
            // Indexed variable: check all indices are simple
            let mut indices = Vec::new();
            for idx in &v.indices {
                match classify_index_expr(idx) {
                    Some(src) => indices.push(src),
                    None => return None,
                }
            }
            Some(Some(FieldMapping::IndexedVariable {
                name: v.name.clone(),
                indices,
            }))
        }
        Expr::Number(n) => Some(Some(FieldMapping::Constant(*n))),
        Expr::DesiredNumber(n) => Some(Some(FieldMapping::DesiredConstant(*n))),
        // InconsistentVar with no indices: treat like a variable
        Expr::InconsistentVar(v) if v.indices.is_empty() => {
            Some(Some(FieldMapping::Variable(v.name.clone())))
        }
        _ => None,
    }
}

/// Classify a list body item expression for fast-path.
/// Returns None if the expression is too complex.
fn classify_list_item(item: &Expr) -> Option<ListBodyMapping> {
    match item {
        Expr::Variable(v) => {
            let mut indices = Vec::new();
            for idx in &v.indices {
                match classify_index_expr(idx) {
                    Some(src) => indices.push(src),
                    None => return None,
                }
            }
            Some(ListBodyMapping::IndexedVar {
                name: v.name.clone(),
                indices,
            })
        }
        Expr::Number(n) => Some(ListBodyMapping::Constant(*n)),
        _ => None,
    }
}

/// Classify an index expression as either a loop variable reference or constant.
fn classify_index_expr(expr: &Expr) -> Option<IndexSource> {
    match expr {
        Expr::Variable(v) if v.indices.is_empty() => {
            Some(IndexSource::LoopVar(v.name.clone()))
        }
        Expr::Number(n) => Some(IndexSource::Constant(*n as i64)),
        _ => None,
    }
}

/// Recursively flatten a list body (handling inner loops) into ListBodyMappings.
/// Returns None if any item is too complex.
fn flatten_list_body(items: &[ListItem]) -> Option<Vec<ListBodyMapping>> {
    // We only handle the simple case: a flat list of values, possibly
    // wrapped in a single loop level. We don't try to handle nested loops
    // or padding in the fast path.
    let mut result = Vec::new();
    for item in items {
        match item {
            ListItem::Value(expr) => {
                result.push(classify_list_item(expr)?);
            }
            ListItem::Loop { body, var: _, start: _, stop: _ } => {
                // We can handle inner loops if all body items are simple
                // But we don't flatten them - we handle them in the executor.
                // For detection, just check all items are classifiable.
                for inner in body {
                    match inner {
                        ListItem::Value(expr) => {
                            classify_list_item(expr)?;
                        }
                        _ => return None, // nested loops or padding inside loop
                    }
                }
                // If we get here, the inner loop body is simple enough.
                // But we return None for now since the executor handles
                // the original AST directly.
                return None;
            }
            ListItem::Padding => return None,
        }
    }
    Some(result)
}

/// Classify an ExtVarName into a FastSectionName if its indices are simple.
fn classify_section_name(name: &ExtVarName) -> Option<FastSectionName> {
    let mut indices = Vec::new();
    for idx in &name.indices {
        match classify_index_expr(idx) {
            Some(src) => indices.push(src),
            None => return None,
        }
    }
    Some(FastSectionName {
        name: name.name.clone(),
        indices,
    })
}

/// Check if a for-loop body qualifies for fast-path processing.
/// The body must be exactly one TAB1 or LIST record with simple field mappings.
pub fn detect_fast_pattern(body: &[RecipeNode]) -> Option<FastPattern> {
    if body.len() != 1 {
        return None;
    }

    match &body[0] {
        RecipeNode::Tab1 {
            ctrl: _,
            fields,
            x_var,
            y_var,
            table_name,
        } => {
            // Check that the first 4 fields are simple
            let mut field_maps: [Option<FieldMapping>; 4] = [None, None, None, None];
            for i in 0..4 {
                match classify_field(&fields[i]) {
                    Some(mapping) => field_maps[i] = mapping,
                    None => return None,
                }
            }
            // x_var and y_var must be simple (no indices)
            if !x_var.indices.is_empty() || !y_var.indices.is_empty() {
                return None;
            }
            // table_name: classify if present
            let fast_table_name = if let Some(tn) = table_name {
                Some(classify_section_name(tn)?)
            } else {
                None
            };
            Some(FastPattern::Tab1Loop {
                field_maps,
                x_var: x_var.name.clone(),
                y_var: y_var.name.clone(),
                table_name: fast_table_name,
            })
        }
        RecipeNode::List {
            ctrl: _,
            fields,
            body: list_body,
            list_name,
        } => {
            // Check all 6 header fields are simple
            let mut field_maps: [Option<FieldMapping>; 6] = [None, None, None, None, None, None];
            for i in 0..6 {
                match classify_field(&fields[i]) {
                    Some(mapping) => field_maps[i] = mapping,
                    None => return None,
                }
            }
            // Check list body items
            let body_vars = flatten_list_body(list_body)?;
            // list_name: classify if present
            let fast_list_name = if let Some(ln) = list_name {
                Some(classify_section_name(ln)?)
            } else {
                None
            };
            Some(FastPattern::ListLoop {
                field_maps,
                body_vars,
                list_name: fast_list_name,
            })
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Fast-path executors
// ---------------------------------------------------------------------------

/// Resolve an IndexSource to a concrete i64 using loop_vars.
#[inline]
fn resolve_index(src: &IndexSource, loop_vars: &std::collections::HashMap<String, i64>) -> EndfResult<i64> {
    match src {
        IndexSource::LoopVar(lv) => {
            loop_vars.get(lv.as_str())
                .copied()
                .ok_or_else(|| EndfError::VariableNotFound {
                    name: lv.clone(),
                })
        }
        IndexSource::Constant(c) => Ok(*c),
    }
}

/// A resolved field assignment ready to be applied to a scope.
enum ResolvedFieldAssignment<'a> {
    Scalar { name: &'a str, val: EndfValue },
    Indexed { name: &'a str, indices: Vec<i64>, val: EndfValue },
}

/// Pre-resolve header field mappings into concrete assignments.
/// This separates index resolution (needs loop_vars) from scope mutation.
fn resolve_header_fields<'a>(
    field_maps: &'a [Option<FieldMapping>],
    field_values: &[f64],
    loop_vars: &std::collections::HashMap<String, i64>,
    parse_opts: &ParseOpts,
) -> EndfResult<Vec<ResolvedFieldAssignment<'a>>> {
    let mut assignments = Vec::new();
    for (mapping, &value) in field_maps.iter().zip(field_values.iter()) {
        if let Some(ref fm) = mapping {
            match fm {
                FieldMapping::Variable(name) => {
                    let val = EndfValue::from_f64(value);
                    assignments.push(ResolvedFieldAssignment::Scalar {
                        name: name.as_str(),
                        val,
                    });
                }
                FieldMapping::IndexedVariable { name, indices } => {
                    let val = EndfValue::from_f64(value);
                    let mut resolved = Vec::with_capacity(indices.len());
                    for idx_src in indices {
                        resolved.push(resolve_index(idx_src, loop_vars)?);
                    }
                    assignments.push(ResolvedFieldAssignment::Indexed {
                        name: name.as_str(),
                        indices: resolved,
                        val,
                    });
                }
                FieldMapping::Constant(expected) => {
                    if *expected != value {
                        if *expected == 0.0 && parse_opts.ignore_zero_mismatch {
                            // tolerated
                        } else {
                            return Err(EndfError::NumberMismatch {
                                expected: *expected,
                                got: value,
                                field: "fast_path".to_string(),
                            });
                        }
                    }
                }
                FieldMapping::DesiredConstant(expected) => {
                    if *expected != value && !parse_opts.ignore_number_mismatch {
                        return Err(EndfError::NumberMismatch {
                            expected: *expected,
                            got: value,
                            field: "fast_path".to_string(),
                        });
                    }
                }
            }
        }
    }
    Ok(assignments)
}

/// Apply pre-resolved field assignments to a mutable scope.
fn apply_assignments(
    assignments: Vec<ResolvedFieldAssignment>,
    scope: &mut EndfValue,
    parse_opts: &ParseOpts,
) -> EndfResult<()> {
    for assignment in assignments {
        match assignment {
            ResolvedFieldAssignment::Scalar { name, val } => {
                scope.insert(name, val);
            }
            ResolvedFieldAssignment::Indexed { name, indices, val } => {
                set_indexed_value(scope, name, &indices, val, parse_opts)?;
            }
        }
    }
    Ok(())
}

/// Read a TAB1 record from `&[String]` lines at the given offset.
/// Returns (ContRecord, Tab1Body, new_offset).
fn read_tab1_from_strings(
    lines: &[String],
    ofs: usize,
    read_opts: &ReadOpts,
) -> EndfResult<(records::ContRecord, records::Tab1Body, usize)> {
    // Read header line
    let (cont, _ctrl) = records::read_cont(&lines[ofs], read_opts)?;
    let nr = cont.n1 as usize;
    let np = cont.n2 as usize;
    // Build a small slice just for the body lines needed.
    // Body needs: ceil(2*NR/6) + ceil(2*NP/6) lines
    let body_lines_needed = ((2 * nr + 5) / 6) + ((2 * np + 5) / 6);
    let end = (ofs + 1 + body_lines_needed).min(lines.len());
    let line_refs: Vec<&str> = lines[ofs + 1..end].iter().map(|s| s.as_str()).collect();
    let (body, body_end) = records::read_tab1_body(&line_refs, 0, nr, np, read_opts)?;
    Ok((cont, body, ofs + 1 + body_end))
}

/// Read a LIST record from `&[String]` lines at the given offset.
fn read_list_from_strings(
    lines: &[String],
    ofs: usize,
    read_opts: &ReadOpts,
) -> EndfResult<(records::ContRecord, Vec<f64>, usize)> {
    let (cont, _ctrl) = records::read_cont(&lines[ofs], read_opts)?;
    let npl = cont.n1 as usize;
    if npl == 0 {
        return Ok((cont, Vec::new(), ofs + 1));
    }
    let body_lines_needed = (npl + 5) / 6;
    let end = (ofs + 1 + body_lines_needed).min(lines.len());
    let line_refs: Vec<&str> = lines[ofs + 1..end].iter().map(|s| s.as_str()).collect();
    let (vals, body_end) = records::read_endf_numbers(&line_refs, npl, 0, read_opts)?;
    Ok((cont, vals, ofs + 1 + body_end))
}

/// Resolve a FastSectionName to a sequence of EndfKeys using loop_vars.
/// Simple names produce `[Str(name)]`; indexed names like `foo[i]`
/// produce `[Str("foo"), Int(i)]`.
fn resolve_section_keys(
    fsn: &FastSectionName,
    loop_vars: &std::collections::HashMap<String, i64>,
) -> EndfResult<Vec<EndfKey>> {
    let mut keys = vec![EndfKey::Str(fsn.name.clone())];
    for idx_src in &fsn.indices {
        let idx = resolve_index(idx_src, loop_vars)?;
        keys.push(EndfKey::Int(idx));
    }
    Ok(keys)
}

/// Store table data (NBT, INT, x, y) into the current scope.
fn store_tab1_table_data(
    scope: &mut EndfValue,
    body: &records::Tab1Body,
    x_var: &str,
    y_var: &str,
) {
    let nbt_arr: Vec<Option<EndfValue>> =
        body.nbt.iter().map(|&v| Some(EndfValue::Int(v))).collect();
    let int_arr: Vec<Option<EndfValue>> =
        body.int.iter().map(|&v| Some(EndfValue::Int(v))).collect();
    let x_arr: Vec<Option<EndfValue>> =
        body.x.iter().map(|&v| Some(EndfValue::Float(v))).collect();
    let y_arr: Vec<Option<EndfValue>> =
        body.y.iter().map(|&v| Some(EndfValue::Float(v))).collect();
    scope.insert("NBT", EndfValue::List(nbt_arr));
    scope.insert("INT", EndfValue::List(int_arr));
    scope.insert(x_var, EndfValue::List(x_arr));
    scope.insert(y_var, EndfValue::List(y_arr));
}

/// Execute a fast-path TAB1 loop.
pub fn execute_fast_tab1_loop(
    field_maps: &[Option<FieldMapping>; 4],
    x_var: &str,
    y_var: &str,
    table_name: &Option<FastSectionName>,
    loop_var: &str,
    start_i: i64,
    stop_i: i64,
    state: &mut InterpreterState,
    read_opts: &ReadOpts,
    parse_opts: &ParseOpts,
) -> EndfResult<()> {
    for i in start_i..=stop_i {
        state.loop_vars.insert(loop_var.to_string(), i);

        // Read TAB1 record directly, building only a small line slice
        let (cont, body, new_ofs) =
            read_tab1_from_strings(&state.lines, state.ofs, read_opts)?;
        state.ofs = new_ofs;

        // Store header fields directly into current scope
        let header_values = [
            cont.c1,
            cont.c2,
            cont.l1 as f64,
            cont.l2 as f64,
        ];
        let assignments = resolve_header_fields(
            field_maps, &header_values, &state.loop_vars, parse_opts,
        )?;
        apply_assignments(assignments, state.current_scope_mut(), parse_opts)?;

        // Handle table section
        if let Some(ref tn) = table_name {
            let keys = resolve_section_keys(tn, &state.loop_vars)?;
            let depth = keys.len();
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

            store_tab1_table_data(state.current_scope_mut(), &body, x_var, y_var);

            let new_len = state.scope_path.len() - depth;
            state.scope_path.truncate(new_len);
        } else {
            store_tab1_table_data(state.current_scope_mut(), &body, x_var, y_var);
        }
    }
    state.loop_vars.remove(loop_var);
    Ok(())
}

/// Execute a fast-path LIST loop.
pub fn execute_fast_list_loop(
    field_maps: &[Option<FieldMapping>; 6],
    body_vars: &[ListBodyMapping],
    list_name: &Option<FastSectionName>,
    loop_var: &str,
    start_i: i64,
    stop_i: i64,
    state: &mut InterpreterState,
    read_opts: &ReadOpts,
    parse_opts: &ParseOpts,
) -> EndfResult<()> {
    for i in start_i..=stop_i {
        state.loop_vars.insert(loop_var.to_string(), i);

        // Read LIST record directly, building only a small line slice
        let (cont, vals, new_ofs) =
            read_list_from_strings(&state.lines, state.ofs, read_opts)?;
        state.ofs = new_ofs;

        // Store all 6 header fields
        let header_values = [
            cont.c1,
            cont.c2,
            cont.l1 as f64,
            cont.l2 as f64,
            cont.n1 as f64,
            cont.n2 as f64,
        ];
        let assignments = resolve_header_fields(
            field_maps, &header_values, &state.loop_vars, parse_opts,
        )?;
        apply_assignments(assignments, state.current_scope_mut(), parse_opts)?;

        // Enter list section if named
        let list_section_depth = if let Some(ref ln) = list_name {
            let keys = resolve_section_keys(ln, &state.loop_vars)?;
            let depth = keys.len();
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
            depth
        } else {
            0
        };

        // Store list body values
        if !body_vars.is_empty() {
            // First, resolve all indices while we can borrow loop_vars immutably.
            // Collect (name, resolved_indices, value) tuples.
            let mut assignments: Vec<(&str, Vec<i64>, EndfValue)> =
                Vec::with_capacity(body_vars.len());

            for (val_idx, mapping) in body_vars.iter().enumerate() {
                if val_idx >= vals.len() {
                    return Err(EndfError::MoreListElementsExpected {
                        expected: val_idx + 1,
                        got: vals.len(),
                    });
                }
                match mapping {
                    ListBodyMapping::IndexedVar { name, indices } => {
                        // All LIST body values are floats by design.
                        let val = EndfValue::Float(vals[val_idx]);
                        if indices.is_empty() {
                            assignments.push((name.as_str(), Vec::new(), val));
                        } else {
                            let mut resolved: Vec<i64> = Vec::with_capacity(indices.len());
                            for idx_src in indices {
                                match idx_src {
                                    IndexSource::LoopVar(lv) => {
                                        let lv_val = state.loop_vars.get(lv.as_str())
                                            .copied()
                                            .ok_or_else(|| EndfError::VariableNotFound {
                                                name: lv.clone(),
                                            })?;
                                        resolved.push(lv_val);
                                    }
                                    IndexSource::Constant(c) => {
                                        resolved.push(*c);
                                    }
                                }
                            }
                            assignments.push((name.as_str(), resolved, val));
                        }
                    }
                    ListBodyMapping::Constant(_expected) => {
                        // Skip validation for speed.
                    }
                }
            }

            // Pre-create top-level containers for indexed variables to
            // avoid repeated contains_key checks inside set_indexed_value.
            {
                let scope = state.current_scope_mut();
                let mut created: HashSet<&str> = HashSet::new();
                for (name, indices, _) in &assignments {
                    if !indices.is_empty() && created.insert(name) {
                        if !scope.contains_key(*name) {
                            let container = match parse_opts.array_type {
                                crate::options::ArrayType::Dict => EndfValue::new_dict(),
                                crate::options::ArrayType::List => EndfValue::new_list(),
                            };
                            scope.insert(*name, container);
                        }
                    }
                }
            }

            // Apply all assignments.
            let scope = state.current_scope_mut();
            for (name, indices, val) in assignments {
                if indices.is_empty() {
                    scope.insert(name, val);
                } else {
                    set_indexed_value_precreated(scope, name, &indices, val, parse_opts)?;
                }
            }

            // Check for unconsumed values
            if body_vars.len() < vals.len() {
                return Err(EndfError::UnconsumedListElements {
                    remaining: vals.len() - body_vars.len(),
                });
            }
        }

        // Exit list section if entered
        if list_section_depth > 0 {
            let new_len = state.scope_path.len() - list_section_depth;
            state.scope_path.truncate(new_len);
        }
    }
    state.loop_vars.remove(loop_var);
    Ok(())
}

/// Set a value at a nested index path in the data dictionary.
/// This is a simplified version of set_var_value for the fast path
/// where indices are already resolved to i64 values.
fn set_indexed_value(
    scope: &mut EndfValue,
    name: &str,
    indices: &[i64],
    value: EndfValue,
    opts: &ParseOpts,
) -> EndfResult<()> {
    use crate::options::ArrayType;

    if !scope.contains_key(name) {
        let container = match opts.array_type {
            ArrayType::Dict => EndfValue::new_dict(),
            ArrayType::List => EndfValue::new_list(),
        };
        scope.insert(name, container);
    }

    let mut current = scope.get_mut(name).unwrap();

    for (i, &idx) in indices.iter().enumerate() {
        let is_last = i == indices.len() - 1;

        match current {
            EndfValue::Dict(ref mut d) => {
                let key = EndfKey::Int(idx);
                if is_last {
                    d.insert(key, value);
                    return Ok(());
                }
                if !d.contains_key(&key) {
                    let container = match opts.array_type {
                        ArrayType::Dict => EndfValue::new_dict(),
                        ArrayType::List => EndfValue::new_list(),
                    };
                    d.insert(key.clone(), container);
                }
                current = d.get_mut(&key).unwrap();
            }
            EndfValue::List(ref mut l) => {
                let uidx = idx as usize;
                if is_last {
                    while l.len() <= uidx {
                        l.push(None);
                    }
                    l[uidx] = Some(value);
                    return Ok(());
                }
                while l.len() <= uidx {
                    l.push(None);
                }
                if l[uidx].is_none() {
                    let container = match opts.array_type {
                        ArrayType::Dict => EndfValue::new_dict(),
                        ArrayType::List => EndfValue::new_list(),
                    };
                    l[uidx] = Some(container);
                }
                current = l[uidx].as_mut().unwrap();
            }
            _ => {
                return Err(EndfError::VariableNotFound {
                    name: name.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Like set_indexed_value but assumes top-level container already exists.
fn set_indexed_value_precreated(
    scope: &mut EndfValue,
    name: &str,
    indices: &[i64],
    value: EndfValue,
    opts: &ParseOpts,
) -> EndfResult<()> {
    use crate::options::ArrayType;

    let mut current = scope.get_mut(name).unwrap();

    for (i, &idx) in indices.iter().enumerate() {
        let is_last = i == indices.len() - 1;

        match current {
            EndfValue::Dict(ref mut d) => {
                let key = EndfKey::Int(idx);
                if is_last {
                    d.insert(key, value);
                    return Ok(());
                }
                if !d.contains_key(&key) {
                    let container = match opts.array_type {
                        ArrayType::Dict => EndfValue::new_dict(),
                        ArrayType::List => EndfValue::new_list(),
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
                    l[uidx] = Some(value);
                    return Ok(());
                }
                if l[uidx].is_none() {
                    let container = match opts.array_type {
                        ArrayType::Dict => EndfValue::new_dict(),
                        ArrayType::List => EndfValue::new_list(),
                    };
                    l[uidx] = Some(container);
                }
                current = l[uidx].as_mut().unwrap();
            }
            _ => {
                return Err(EndfError::IndexNotFound {
                    name: name.to_string(),
                    indices: indices.to_vec(),
                });
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_simple_variable() {
        let expr = Expr::Variable(ExtVarName::simple("ZAP"));
        let result = classify_field(&expr);
        assert!(result.is_some());
        match result.unwrap() {
            Some(FieldMapping::Variable(name)) => assert_eq!(name, "ZAP"),
            _ => panic!("expected Variable mapping"),
        }
    }

    #[test]
    fn test_classify_constant() {
        let expr = Expr::Number(0.0);
        let result = classify_field(&expr);
        assert!(result.is_some());
        match result.unwrap() {
            Some(FieldMapping::Constant(v)) => assert_eq!(v, 0.0),
            _ => panic!("expected Constant mapping"),
        }
    }

    #[test]
    fn test_classify_complex_expr_rejected() {
        let expr = Expr::Add(
            Box::new(Expr::Number(2.0)),
            Box::new(Expr::Variable(ExtVarName::simple("X"))),
        );
        let result = classify_field(&expr);
        assert!(result.is_none());
    }

    #[test]
    fn test_classify_indexed_variable_accepted() {
        let expr = Expr::Variable(ExtVarName::with_indices(
            "X",
            vec![Expr::Variable(ExtVarName::simple("i"))],
        ));
        let result = classify_field(&expr);
        assert!(result.is_some());
        match result.unwrap() {
            Some(FieldMapping::IndexedVariable { name, indices }) => {
                assert_eq!(name, "X");
                assert_eq!(indices.len(), 1);
                assert!(matches!(indices[0], IndexSource::LoopVar(ref s) if s == "i"));
            }
            _ => panic!("expected IndexedVariable mapping"),
        }
    }

    #[test]
    fn test_classify_complex_indexed_rejected() {
        // Complex index expression like X[i+1] should be rejected
        let expr = Expr::Variable(ExtVarName::with_indices(
            "X",
            vec![Expr::Add(
                Box::new(Expr::Variable(ExtVarName::simple("i"))),
                Box::new(Expr::Number(1.0)),
            )],
        ));
        let result = classify_field(&expr);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_tab1_pattern() {
        let body = vec![RecipeNode::Tab1 {
            ctrl: CtrlSpec {
                mat: CtrlField::Symbolic,
                mf: CtrlField::Literal(6),
                mt: CtrlField::Symbolic,
            },
            fields: [
                Expr::Variable(ExtVarName::simple("ZAP")),
                Expr::Variable(ExtVarName::simple("AWP")),
                Expr::Variable(ExtVarName::simple("LIP")),
                Expr::Variable(ExtVarName::simple("LAW")),
                Expr::Variable(ExtVarName::simple("NR")),
                Expr::Variable(ExtVarName::simple("NP")),
            ],
            x_var: ExtVarName::simple("Eint"),
            y_var: ExtVarName::simple("yi"),
            table_name: Some(ExtVarName::simple("yields")),
        }];
        let pattern = detect_fast_pattern(&body);
        assert!(pattern.is_some());
        match pattern.unwrap() {
            FastPattern::Tab1Loop {
                field_maps,
                x_var,
                y_var,
                table_name,
            } => {
                assert_eq!(x_var, "Eint");
                assert_eq!(y_var, "yi");
                assert_eq!(table_name.as_ref().map(|n| n.name.as_str()), Some("yields"));
                // All 4 fields should be Variable mappings
                for fm in &field_maps {
                    assert!(matches!(fm, Some(FieldMapping::Variable(_))));
                }
            }
            _ => panic!("expected Tab1Loop"),
        }
    }

    #[test]
    fn test_detect_non_qualifying_multi_node_body() {
        let body = vec![
            RecipeNode::Comment("test".to_string()),
            RecipeNode::Send,
        ];
        assert!(detect_fast_pattern(&body).is_none());
    }
}
