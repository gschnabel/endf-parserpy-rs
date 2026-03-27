use std::collections::HashMap;
use crate::error::{EndfError, EndfResult};
use crate::recipe::ast::*;
use crate::value::EndfValue;
use super::state::{InterpreterState, RwMode};

/// Trait for evaluating arithmetic expressions.
/// This allows the control flow module to be independent of the expression
/// evaluator implementation, which lives in a separate module.
pub trait ExprEvaluator {
    /// Evaluate an expression to a known f64 value.
    /// The scope_chain is ordered innermost first.
    fn eval_expr_known(
        &self,
        expr: &Expr,
        scope_chain: &[&EndfValue],
        loop_vars: &HashMap<String, i64>,
        abbreviations: &HashMap<String, Expr>,
    ) -> EndfResult<f64>;
}

/// Execute a for loop.
///
/// Evaluates `start` and `stop` expressions, then iterates `var` from
/// `start` to `stop` (inclusive), executing `body` at each iteration.
pub fn execute_for_loop(
    var: &str,
    start: &Expr,
    stop: &Expr,
    body: &[RecipeNode],
    state: &mut InterpreterState,
    evaluator: &dyn ExprEvaluator,
    run_body: &mut dyn FnMut(&[RecipeNode], &mut InterpreterState) -> EndfResult<()>,
) -> EndfResult<()> {
    let scope_chain = state.scope_chain();
    let start_val = evaluator.eval_expr_known(
        start,
        &scope_chain,
        &state.loop_vars,
        &state.abbreviations,
    )?;
    let stop_val = evaluator.eval_expr_known(
        stop,
        &scope_chain,
        &state.loop_vars,
        &state.abbreviations,
    )?;
    drop(scope_chain);

    let start_i = start_val as i64;
    let stop_i = stop_val as i64;

    // Validate that bounds are actually integers
    if (start_i as f64) != start_val || (stop_i as f64) != stop_val {
        return Err(EndfError::LoopVariableError {
            name: var.to_string(),
        });
    }

    for i in start_i..=stop_i {
        state.loop_vars.insert(var.to_string(), i);
        run_body(body, state)?;
    }
    state.loop_vars.remove(var);
    Ok(())
}

/// Execute a repeat-until loop.
///
/// If `var` is provided, initializes the loop counter variable to the
/// given expression value. Then repeatedly executes `body`, evaluates
/// the `until` condition, and breaks when the condition is true.
/// The counter variable is incremented by 1 after each iteration.
pub fn execute_repeat_loop(
    var: &Option<(String, Box<Expr>)>,
    body: &[RecipeNode],
    until: &BoolExpr,
    state: &mut InterpreterState,
    evaluator: &dyn ExprEvaluator,
    run_body: &mut dyn FnMut(&[RecipeNode], &mut InterpreterState) -> EndfResult<()>,
) -> EndfResult<()> {
    // Initialize loop variable if present
    let var_name = if let Some((name, init_expr)) = var {
        let scope_chain = state.scope_chain();
        let init_val = evaluator.eval_expr_known(
            init_expr,
            &scope_chain,
            &state.loop_vars,
            &state.abbreviations,
        )?;
        drop(scope_chain);
        let init_i = init_val as i64;
        if (init_i as f64) != init_val {
            return Err(EndfError::LoopVariableError {
                name: name.clone(),
            });
        }
        state.loop_vars.insert(name.clone(), init_i);
        Some(name.clone())
    } else {
        None
    };

    loop {
        run_body(body, state)?;

        let truthval = eval_bool(until, state, evaluator, false)?;
        if truthval {
            break;
        }

        // Increment loop variable if present
        if let Some(ref name) = var_name {
            if let Some(counter) = state.loop_vars.get_mut(name) {
                *counter += 1;
            }
        }
    }

    // Clean up loop variable
    if let Some(ref name) = var_name {
        state.loop_vars.remove(name);
    }

    Ok(())
}

/// Evaluate an if/elif/else clause.
///
/// Iterates through branches in order. For branches with lookahead
/// (only in read mode), performs a speculative execution: snapshots state,
/// runs the body in lookahead mode, evaluates the condition, then restores
/// state. If the condition is true, executes the body for real.
///
/// If no branch matches and an else body exists, the else body is executed.
pub fn evaluate_if_clause(
    branches: &[IfBranch],
    else_body: &Option<Vec<RecipeNode>>,
    state: &mut InterpreterState,
    evaluator: &dyn ExprEvaluator,
    run_body: &mut dyn FnMut(&[RecipeNode], &mut InterpreterState) -> EndfResult<()>,
) -> EndfResult<()> {
    for branch in branches {
        let should_lookahead =
            branch.lookahead.is_some() && state.rwmode == RwMode::Read;

        let truthval = if should_lookahead {
            // Snapshot state for speculative execution
            let snapshot = state.snapshot();

            // Evaluate lookahead depth
            let scope_chain = state.scope_chain();
            let depth = evaluator.eval_expr_known(
                branch.lookahead.as_ref().unwrap(),
                &scope_chain,
                &state.loop_vars,
                &state.abbreviations,
            )?;
            drop(scope_chain);
            state.lookahead_depth = Some(depth as usize);

            // Execute body speculatively; ignore UnexpectedControlRecord errors
            let _exec_result = run_body(&branch.body, state);

            // Evaluate condition with the (potentially modified) state
            // Use missing_as_false since variables may not be resolved
            let result = eval_bool(&branch.condition, state, evaluator, true);

            // Restore state regardless of outcome
            state.restore(snapshot);
            result?
        } else {
            // In write mode with lookahead-annotated conditions, use missing_as_false
            // because variables like LCOMP may only exist in some scope paths
            let maf = branch.lookahead.is_some() && state.rwmode == RwMode::Write;
            eval_bool(&branch.condition, state, evaluator, maf)?
        };

        if truthval {
            run_body(&branch.body, state)?;
            return Ok(());
        }
    }

    // No branch matched; try else
    if let Some(else_b) = else_body {
        run_body(else_b, state)?;
    }
    Ok(())
}

/// Evaluate a boolean expression.
///
/// When `missing_as_false` is true, any variable-not-found errors cause
/// the comparison to return false instead of propagating the error.
/// This is used during lookahead evaluation.
pub fn eval_bool(
    expr: &BoolExpr,
    state: &InterpreterState,
    evaluator: &dyn ExprEvaluator,
    missing_as_false: bool,
) -> EndfResult<bool> {
    match expr {
        BoolExpr::Comparison { left, op, right } => {
            let scope_chain = state.scope_chain();
            let left_val = match evaluator.eval_expr_known(
                left,
                &scope_chain,
                &state.loop_vars,
                &state.abbreviations,
            ) {
                Ok(v) => v,
                Err(_) if missing_as_false => return Ok(false),
                Err(e) => return Err(e),
            };
            let right_val = match evaluator.eval_expr_known(
                right,
                &scope_chain,
                &state.loop_vars,
                &state.abbreviations,
            ) {
                Ok(v) => v,
                Err(_) if missing_as_false => return Ok(false),
                Err(e) => return Err(e),
            };
            Ok(match op {
                CmpOp::Eq => left_val == right_val,
                CmpOp::Ne => left_val != right_val,
                CmpOp::Lt => left_val < right_val,
                CmpOp::Gt => left_val > right_val,
                CmpOp::Le => left_val <= right_val,
                CmpOp::Ge => left_val >= right_val,
            })
        }
        BoolExpr::And(a, b) => {
            if !eval_bool(a, state, evaluator, missing_as_false)? {
                return Ok(false);
            }
            eval_bool(b, state, evaluator, missing_as_false)
        }
        BoolExpr::Or(a, b) => {
            if eval_bool(a, state, evaluator, missing_as_false)? {
                return Ok(true);
            }
            eval_bool(b, state, evaluator, missing_as_false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple evaluator that resolves Number literals and loop variables.
    struct TestEvaluator;

    impl ExprEvaluator for TestEvaluator {
        fn eval_expr_known(
            &self,
            expr: &Expr,
            _scope_chain: &[&EndfValue],
            loop_vars: &HashMap<String, i64>,
            _abbreviations: &HashMap<String, Expr>,
        ) -> EndfResult<f64> {
            match expr {
                Expr::Number(v) => Ok(*v),
                Expr::Variable(ext) if ext.indices.is_empty() => {
                    if let Some(&val) = loop_vars.get(&ext.name) {
                        Ok(val as f64)
                    } else {
                        Err(EndfError::VariableNotFound {
                            name: ext.name.clone(),
                        })
                    }
                }
                Expr::Add(a, b) => {
                    let va = self.eval_expr_known(a, _scope_chain, loop_vars, _abbreviations)?;
                    let vb = self.eval_expr_known(b, _scope_chain, loop_vars, _abbreviations)?;
                    Ok(va + vb)
                }
                _ => Err(EndfError::VariableNotFound {
                    name: "unsupported_expr".to_string(),
                }),
            }
        }
    }

    fn make_state() -> InterpreterState {
        InterpreterState::new_read(vec![])
    }

    // ---------------------------------------------------------------
    // For loop tests
    // ---------------------------------------------------------------

    #[test]
    fn test_for_loop_basic() {
        let mut state = make_state();
        let evaluator = TestEvaluator;
        let start = Expr::Number(1.0);
        let stop = Expr::Number(3.0);
        let body: Vec<RecipeNode> = vec![];

        let mut iterations = Vec::new();
        let iterations_ref = &mut iterations;

        execute_for_loop(
            "i",
            &start,
            &stop,
            &body,
            &mut state,
            &evaluator,
            &mut |_body, st| {
                iterations_ref.push(st.loop_vars["i"]);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(iterations, vec![1, 2, 3]);
        // Loop var should be cleaned up
        assert!(!state.loop_vars.contains_key("i"));
    }

    #[test]
    fn test_for_loop_empty_range() {
        let mut state = make_state();
        let evaluator = TestEvaluator;
        let start = Expr::Number(5.0);
        let stop = Expr::Number(3.0);
        let body: Vec<RecipeNode> = vec![];

        let mut count = 0;
        let count_ref = &mut count;

        execute_for_loop(
            "i",
            &start,
            &stop,
            &body,
            &mut state,
            &evaluator,
            &mut |_body, _st| {
                *count_ref += 1;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_for_loop_non_integer_bounds() {
        let mut state = make_state();
        let evaluator = TestEvaluator;
        let start = Expr::Number(1.5);
        let stop = Expr::Number(3.0);
        let body: Vec<RecipeNode> = vec![];

        let result = execute_for_loop(
            "i",
            &start,
            &stop,
            &body,
            &mut state,
            &evaluator,
            &mut |_body, _st| Ok(()),
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            EndfError::LoopVariableError { name } => assert_eq!(name, "i"),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    // ---------------------------------------------------------------
    // Boolean expression evaluation tests
    // ---------------------------------------------------------------

    fn cmp_expr(left: f64, op: CmpOp, right: f64) -> BoolExpr {
        BoolExpr::Comparison {
            left: Box::new(Expr::Number(left)),
            op,
            right: Box::new(Expr::Number(right)),
        }
    }

    #[test]
    fn test_eval_bool_eq() {
        let state = make_state();
        let evaluator = TestEvaluator;

        assert!(eval_bool(&cmp_expr(5.0, CmpOp::Eq, 5.0), &state, &evaluator, false).unwrap());
        assert!(!eval_bool(&cmp_expr(5.0, CmpOp::Eq, 6.0), &state, &evaluator, false).unwrap());
    }

    #[test]
    fn test_eval_bool_ne() {
        let state = make_state();
        let evaluator = TestEvaluator;

        assert!(eval_bool(&cmp_expr(5.0, CmpOp::Ne, 6.0), &state, &evaluator, false).unwrap());
        assert!(!eval_bool(&cmp_expr(5.0, CmpOp::Ne, 5.0), &state, &evaluator, false).unwrap());
    }

    #[test]
    fn test_eval_bool_lt_gt() {
        let state = make_state();
        let evaluator = TestEvaluator;

        assert!(eval_bool(&cmp_expr(3.0, CmpOp::Lt, 5.0), &state, &evaluator, false).unwrap());
        assert!(!eval_bool(&cmp_expr(5.0, CmpOp::Lt, 3.0), &state, &evaluator, false).unwrap());
        assert!(eval_bool(&cmp_expr(5.0, CmpOp::Gt, 3.0), &state, &evaluator, false).unwrap());
        assert!(!eval_bool(&cmp_expr(3.0, CmpOp::Gt, 5.0), &state, &evaluator, false).unwrap());
    }

    #[test]
    fn test_eval_bool_le_ge() {
        let state = make_state();
        let evaluator = TestEvaluator;

        assert!(eval_bool(&cmp_expr(3.0, CmpOp::Le, 3.0), &state, &evaluator, false).unwrap());
        assert!(eval_bool(&cmp_expr(3.0, CmpOp::Le, 5.0), &state, &evaluator, false).unwrap());
        assert!(!eval_bool(&cmp_expr(5.0, CmpOp::Le, 3.0), &state, &evaluator, false).unwrap());

        assert!(eval_bool(&cmp_expr(3.0, CmpOp::Ge, 3.0), &state, &evaluator, false).unwrap());
        assert!(eval_bool(&cmp_expr(5.0, CmpOp::Ge, 3.0), &state, &evaluator, false).unwrap());
        assert!(!eval_bool(&cmp_expr(3.0, CmpOp::Ge, 5.0), &state, &evaluator, false).unwrap());
    }

    #[test]
    fn test_eval_bool_and() {
        let state = make_state();
        let evaluator = TestEvaluator;

        let both_true = BoolExpr::And(
            Box::new(cmp_expr(1.0, CmpOp::Eq, 1.0)),
            Box::new(cmp_expr(2.0, CmpOp::Eq, 2.0)),
        );
        assert!(eval_bool(&both_true, &state, &evaluator, false).unwrap());

        let left_false = BoolExpr::And(
            Box::new(cmp_expr(1.0, CmpOp::Eq, 2.0)),
            Box::new(cmp_expr(2.0, CmpOp::Eq, 2.0)),
        );
        assert!(!eval_bool(&left_false, &state, &evaluator, false).unwrap());

        let right_false = BoolExpr::And(
            Box::new(cmp_expr(1.0, CmpOp::Eq, 1.0)),
            Box::new(cmp_expr(2.0, CmpOp::Eq, 3.0)),
        );
        assert!(!eval_bool(&right_false, &state, &evaluator, false).unwrap());
    }

    #[test]
    fn test_eval_bool_or() {
        let state = make_state();
        let evaluator = TestEvaluator;

        let either_true = BoolExpr::Or(
            Box::new(cmp_expr(1.0, CmpOp::Eq, 2.0)),
            Box::new(cmp_expr(2.0, CmpOp::Eq, 2.0)),
        );
        assert!(eval_bool(&either_true, &state, &evaluator, false).unwrap());

        let both_false = BoolExpr::Or(
            Box::new(cmp_expr(1.0, CmpOp::Eq, 2.0)),
            Box::new(cmp_expr(3.0, CmpOp::Eq, 4.0)),
        );
        assert!(!eval_bool(&both_false, &state, &evaluator, false).unwrap());
    }

    #[test]
    fn test_eval_bool_missing_as_false() {
        let state = make_state();
        let evaluator = TestEvaluator;

        // Reference to unknown variable
        let expr = BoolExpr::Comparison {
            left: Box::new(Expr::Variable(ExtVarName::simple("unknown"))),
            op: CmpOp::Eq,
            right: Box::new(Expr::Number(1.0)),
        };

        // Without missing_as_false, should error
        assert!(eval_bool(&expr, &state, &evaluator, false).is_err());

        // With missing_as_false, should return false
        assert!(!eval_bool(&expr, &state, &evaluator, true).unwrap());
    }

    // ---------------------------------------------------------------
    // If clause tests
    // ---------------------------------------------------------------

    #[test]
    fn test_if_clause_first_branch_true() {
        let mut state = make_state();
        let evaluator = TestEvaluator;

        let branches = vec![IfBranch {
            condition: cmp_expr(1.0, CmpOp::Eq, 1.0),
            lookahead: None,
            body: vec![],
        }];

        let mut executed = false;
        let executed_ref = &mut executed;

        evaluate_if_clause(
            &branches,
            &None,
            &mut state,
            &evaluator,
            &mut |_body, _st| {
                *executed_ref = true;
                Ok(())
            },
        )
        .unwrap();

        assert!(executed);
    }

    #[test]
    fn test_if_clause_falls_through_to_else() {
        let mut state = make_state();
        let evaluator = TestEvaluator;

        let branches = vec![IfBranch {
            condition: cmp_expr(1.0, CmpOp::Eq, 2.0),
            lookahead: None,
            body: vec![],
        }];

        let else_body = Some(vec![]);
        let mut call_count = 0;
        let call_count_ref = &mut call_count;

        evaluate_if_clause(
            &branches,
            &else_body,
            &mut state,
            &evaluator,
            &mut |_body, _st| {
                *call_count_ref += 1;
                Ok(())
            },
        )
        .unwrap();

        // Should have called run_body once (for the else body)
        assert_eq!(call_count, 1);
    }

    #[test]
    fn test_if_clause_no_match_no_else() {
        let mut state = make_state();
        let evaluator = TestEvaluator;

        let branches = vec![IfBranch {
            condition: cmp_expr(1.0, CmpOp::Eq, 2.0),
            lookahead: None,
            body: vec![],
        }];

        let mut call_count = 0;
        let call_count_ref = &mut call_count;

        evaluate_if_clause(
            &branches,
            &None,
            &mut state,
            &evaluator,
            &mut |_body, _st| {
                *call_count_ref += 1;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(call_count, 0);
    }

    #[test]
    fn test_if_clause_second_branch_matches() {
        let mut state = make_state();
        let evaluator = TestEvaluator;

        let branches = vec![
            IfBranch {
                condition: cmp_expr(1.0, CmpOp::Eq, 2.0), // false
                lookahead: None,
                body: vec![],
            },
            IfBranch {
                condition: cmp_expr(3.0, CmpOp::Eq, 3.0), // true
                lookahead: None,
                body: vec![],
            },
        ];

        let mut call_count = 0;
        let call_count_ref = &mut call_count;

        evaluate_if_clause(
            &branches,
            &None,
            &mut state,
            &evaluator,
            &mut |_body, _st| {
                *call_count_ref += 1;
                Ok(())
            },
        )
        .unwrap();

        // Should only run body once (for the matching branch)
        assert_eq!(call_count, 1);
    }

    // ---------------------------------------------------------------
    // Repeat loop tests
    // ---------------------------------------------------------------

    #[test]
    fn test_repeat_loop_basic() {
        let mut state = make_state();
        let evaluator = TestEvaluator;

        // repeat counter = 0: ... until counter == 2
        let var = Some(("counter".to_string(), Box::new(Expr::Number(0.0))));
        let until = BoolExpr::Comparison {
            left: Box::new(Expr::Variable(ExtVarName::simple("counter"))),
            op: CmpOp::Eq,
            right: Box::new(Expr::Number(2.0)),
        };

        let mut iterations = Vec::new();
        let iterations_ref = &mut iterations;

        execute_repeat_loop(
            &var,
            &[],
            &until,
            &mut state,
            &evaluator,
            &mut |_body, st| {
                iterations_ref.push(st.loop_vars["counter"]);
                Ok(())
            },
        )
        .unwrap();

        // Body runs with counter=0, condition false (0!=2), increment to 1
        // Body runs with counter=1, condition false (1!=2), increment to 2
        // Body runs with counter=2, condition true (2==2), break
        assert_eq!(iterations, vec![0, 1, 2]);
        // Loop var cleaned up
        assert!(!state.loop_vars.contains_key("counter"));
    }
}
