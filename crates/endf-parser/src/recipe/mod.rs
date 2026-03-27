pub mod ast;
pub mod catalogue;

use pest::iterators::{Pair, Pairs};
use pest::Parser;
use pest_derive::Parser;

use crate::error::{EndfError, EndfResult};
use ast::*;

#[derive(Parser)]
#[grammar = "recipe/endf_recipe.pest"]
struct RecipeParser;

/// Parse a recipe string into AST nodes.
pub fn parse_recipe(input: &str) -> EndfResult<Vec<RecipeNode>> {
    let pairs = RecipeParser::parse(Rule::endf_recipe, input)
        .map_err(|e| EndfError::RecipeParse {
            message: e.to_string(),
        })?;

    let recipe_pair = pairs.into_iter().next().unwrap();
    Ok(parse_body(recipe_pair.into_inner()))
}

// ---------------------------------------------------------------------------
// Body / code_token collection
// ---------------------------------------------------------------------------

fn parse_body(pairs: Pairs<Rule>) -> Vec<RecipeNode> {
    let mut nodes = Vec::new();
    for pair in pairs {
        match pair.as_rule() {
            Rule::code_token => nodes.push(parse_code_token(pair)),
            Rule::NEWLINE | Rule::EOI => {} // skip
            _ => {}
        }
    }
    nodes
}

fn parse_code_token(pair: Pair<Rule>) -> RecipeNode {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::endf_line => parse_endf_line(inner),
        Rule::for_loop => parse_for_loop(inner),
        Rule::repeat_loop => parse_repeat_loop(inner),
        Rule::if_clause => parse_if_clause(inner),
        Rule::section => parse_section(inner),
        Rule::abbreviation => parse_abbreviation(inner),
        Rule::comment_block => parse_comment_block(inner),
        _ => unreachable!("unexpected code_token variant: {:?}", inner.as_rule()),
    }
}

fn parse_endf_line(pair: Pair<Rule>) -> RecipeNode {
    let inner = pair
        .into_inner()
        .find(|p| p.as_rule() != Rule::NEWLINE)
        .unwrap();
    match inner.as_rule() {
        Rule::head_or_cont_line => parse_head_or_cont_line(inner),
        Rule::text_line => parse_text_line(inner),
        Rule::dir_line => parse_dir_line(inner),
        Rule::intg_line => parse_intg_line(inner),
        Rule::tab1_line => parse_tab1_line(inner),
        Rule::tab2_line => parse_tab2_line(inner),
        Rule::list_line => parse_list_line(inner),
        Rule::send_line => RecipeNode::Send,
        Rule::stop_line => parse_stop_line(inner),
        _ => unreachable!("unexpected endf_line variant: {:?}", inner.as_rule()),
    }
}

// ---------------------------------------------------------------------------
// Record lines
// ---------------------------------------------------------------------------

fn parse_head_or_cont_line(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let ctrl = parse_ctrl_spec(inner.next().unwrap());
    let fields = parse_record_fields(inner.next().unwrap());
    let subtype_str = inner.next().unwrap().as_str();
    let subtype = if subtype_str == "HEAD" {
        ContSubtype::Head
    } else {
        ContSubtype::Cont
    };
    RecipeNode::HeadOrCont {
        ctrl,
        fields,
        subtype,
    }
}

fn parse_text_line(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let ctrl = parse_ctrl_spec(inner.next().unwrap());
    let text_fields_pair = inner.next().unwrap(); // text_fields
    let placeholders = text_fields_pair
        .into_inner()
        .filter(|p| p.as_rule() == Rule::textplaceholder)
        .map(parse_textplaceholder)
        .collect();
    RecipeNode::Text { ctrl, placeholders }
}

fn parse_textplaceholder(pair: Pair<Rule>) -> TextPlaceholder {
    let mut var = None;
    let mut width = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::extvarname => var = Some(parse_extvarname(p)),
            Rule::TEXTLENGTH => {
                width = Some(p.as_str().parse::<usize>().unwrap());
            }
            _ => {}
        }
    }
    TextPlaceholder { var, width }
}

fn parse_dir_line(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let ctrl = parse_ctrl_spec(inner.next().unwrap());
    let dir_fields_pair = inner.next().unwrap(); // dir_fields
    let exprs: Vec<Expr> = dir_fields_pair
        .into_inner()
        .filter(|p| p.as_rule() == Rule::expr)
        .map(parse_expr)
        .collect();
    let fields = [
        exprs[0].clone(),
        exprs[1].clone(),
        exprs[2].clone(),
        exprs[3].clone(),
    ];
    RecipeNode::Dir { ctrl, fields }
}

fn parse_intg_line(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let ctrl = parse_ctrl_spec(inner.next().unwrap());
    let intg_fields_pair = inner.next().unwrap(); // intg_fields
    let exprs: Vec<Expr> = intg_fields_pair
        .into_inner()
        .filter(|p| p.as_rule() == Rule::expr)
        .map(parse_expr)
        .collect();
    let ndigit_pair = inner.next().unwrap(); // ndigit_expr
    let ndigit = parse_expr(ndigit_pair.into_inner().next().unwrap());
    let fields = [exprs[0].clone(), exprs[1].clone(), exprs[2].clone()];
    RecipeNode::Intg {
        ctrl,
        fields,
        ndigit: Box::new(ndigit),
    }
}

fn parse_tab1_line(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let ctrl = parse_ctrl_spec(inner.next().unwrap());
    let tab1_fields_pair = inner.next().unwrap(); // tab1_fields
    let mut tab1_inner = tab1_fields_pair.into_inner();
    let record_fields = parse_record_fields(tab1_inner.next().unwrap());
    let tab1_def = tab1_inner.next().unwrap(); // tab1_def
    let mut def_inner = tab1_def.into_inner();
    let x_var = parse_extvarname(def_inner.next().unwrap());
    let y_var = parse_extvarname(def_inner.next().unwrap());
    let table_name = inner
        .find(|p| p.as_rule() == Rule::table_name)
        .map(|p| parse_extvarname(p.into_inner().next().unwrap()));
    RecipeNode::Tab1 {
        ctrl,
        fields: record_fields,
        x_var,
        y_var,
        table_name,
    }
}

fn parse_tab2_line(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let ctrl = parse_ctrl_spec(inner.next().unwrap());
    let tab2_fields_pair = inner.next().unwrap(); // tab2_fields
    let mut tab2_inner = tab2_fields_pair.into_inner();
    let record_fields = parse_record_fields(tab2_inner.next().unwrap());
    let tab2_def = tab2_inner.next().unwrap(); // tab2_def
    let z_var = parse_extvarname(tab2_def.into_inner().next().unwrap());
    let table_name = inner
        .find(|p| p.as_rule() == Rule::table_name)
        .map(|p| parse_extvarname(p.into_inner().next().unwrap()));
    RecipeNode::Tab2 {
        ctrl,
        fields: record_fields,
        z_var,
        table_name,
    }
}

fn parse_list_line(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let ctrl = parse_ctrl_spec(inner.next().unwrap());
    let record_fields = parse_record_fields(inner.next().unwrap());
    let list_body_pair = inner.next().unwrap();
    let body = parse_list_body(list_body_pair);
    let list_name = inner
        .find(|p| p.as_rule() == Rule::list_name)
        .map(|p| parse_extvarname(p.into_inner().next().unwrap()));
    RecipeNode::List {
        ctrl,
        fields: record_fields,
        body,
        list_name,
    }
}

fn parse_list_body(pair: Pair<Rule>) -> Vec<ListItem> {
    let mut items = Vec::new();
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::list_expr => items.push(ListItem::Value(parse_list_expr(p))),
            Rule::list_loop => items.push(parse_list_loop(p)),
            Rule::LINEPADDING => items.push(ListItem::Padding),
            Rule::NEWLINE => {}
            _ => {} // commas etc.
        }
    }
    items
}

fn parse_list_loop(pair: Pair<Rule>) -> ListItem {
    let mut inner = pair.into_inner();
    let body_pair = inner.next().unwrap(); // list_body
    let body = parse_list_body(body_pair);
    let for_head = inner.next().unwrap(); // list_for_head
    let mut fh_inner = for_head.into_inner();
    let var = fh_inner.next().unwrap().as_str().to_string();
    let start = Box::new(parse_expr(
        fh_inner.next().unwrap().into_inner().next().unwrap(),
    ));
    let stop = Box::new(parse_expr(
        fh_inner.next().unwrap().into_inner().next().unwrap(),
    ));
    ListItem::Loop {
        body,
        var,
        start,
        stop,
    }
}

fn parse_stop_line(pair: Pair<Rule>) -> RecipeNode {
    let message = pair
        .into_inner()
        .find(|p| p.as_rule() == Rule::escaped_stop_message)
        .and_then(|p| {
            p.into_inner()
                .find(|q| q.as_rule() == Rule::STOP_MESSAGE)
                .map(|q| q.as_str().to_string())
        });
    RecipeNode::Stop { message }
}

fn parse_comment_block(pair: Pair<Rule>) -> RecipeNode {
    // The COMMENT token holds the full text
    let text = pair
        .into_inner()
        .next()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    RecipeNode::Comment(text)
}

// ---------------------------------------------------------------------------
// Control spec
// ---------------------------------------------------------------------------

fn parse_ctrl_spec(pair: Pair<Rule>) -> CtrlSpec {
    let mut inner = pair.into_inner();
    let mat = parse_ctrl_field(inner.next().unwrap());
    let mf = parse_ctrl_field(inner.next().unwrap());
    let mt = parse_ctrl_field(inner.next().unwrap());
    CtrlSpec { mat, mf, mt }
}

fn parse_ctrl_field(pair: Pair<Rule>) -> CtrlField {
    let s = pair.as_str().trim();
    match s {
        "MAT" | "MF" | "MT" => CtrlField::Symbolic,
        _ => CtrlField::Literal(s.parse::<i32>().unwrap()),
    }
}

// ---------------------------------------------------------------------------
// Record fields -> [Expr; 6]
// ---------------------------------------------------------------------------

fn parse_record_fields(pair: Pair<Rule>) -> [Expr; 6] {
    let mut inner = pair.into_inner();
    let mut exprs = Vec::with_capacity(6);
    for p in &mut inner {
        match p.as_rule() {
            Rule::expr => exprs.push(parse_expr(p)),
            Rule::record_expr => exprs.push(parse_record_expr(p)),
            _ => {}
        }
    }
    debug_assert_eq!(exprs.len(), 6, "record_fields must have exactly 6 expressions");
    [
        exprs[0].clone(),
        exprs[1].clone(),
        exprs[2].clone(),
        exprs[3].clone(),
        exprs[4].clone(),
        exprs[5].clone(),
    ]
}

// ---------------------------------------------------------------------------
// Extended variable name
// ---------------------------------------------------------------------------

fn parse_extvarname(pair: Pair<Rule>) -> ExtVarName {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let indices: Vec<Expr> = inner
        .filter(|p| p.as_rule() == Rule::indexquant)
        .map(|p| parse_expr(p.into_inner().next().unwrap()))
        .collect();
    ExtVarName { name, indices }
}

// ---------------------------------------------------------------------------
// Expressions (arithmetic)
// ---------------------------------------------------------------------------

/// Parse `expr = { addpart ~ ((add_op | sub_op) ~ addpart)* }`
fn parse_expr(pair: Pair<Rule>) -> Expr {
    build_additive_expr(pair.into_inner(), |p| parse_addpart(p))
}

/// Parse `record_expr = { record_addpart ~ ((add_op | sub_op) ~ record_addpart)* }`
fn parse_record_expr(pair: Pair<Rule>) -> Expr {
    build_additive_expr(pair.into_inner(), |p| parse_record_addpart(p))
}

/// Parse `list_expr = { list_addpart ~ ((add_op | sub_op) ~ list_addpart)* }`
fn parse_list_expr(pair: Pair<Rule>) -> Expr {
    build_additive_expr(pair.into_inner(), |p| parse_list_addpart(p))
}

/// Shared builder for additive-level expressions (left-associative).
/// Inner pairs: part, op, part, op, part, ...
fn build_additive_expr<F>(mut pairs: Pairs<Rule>, parse_part: F) -> Expr
where
    F: Fn(Pair<Rule>) -> Expr,
{
    let first = pairs.next().unwrap();
    let mut left = parse_part(first);
    while let Some(op_pair) = pairs.next() {
        let right_pair = pairs.next().unwrap();
        let right = parse_part(right_pair);
        left = match op_pair.as_rule() {
            Rule::add_op => Expr::Add(Box::new(left), Box::new(right)),
            Rule::sub_op => Expr::Sub(Box::new(left), Box::new(right)),
            _ => unreachable!("expected add_op or sub_op, got {:?}", op_pair.as_rule()),
        };
    }
    left
}

/// Parse `addpart = { mulpart ~ ((mul_op | div_op | mod_op) ~ mulpart)* }`
fn parse_addpart(pair: Pair<Rule>) -> Expr {
    build_multiplicative_expr(pair.into_inner(), true)
}

/// Parse `record_addpart = { mulpart ~ ((mul_op | mod_op) ~ mulpart)* }`
fn parse_record_addpart(pair: Pair<Rule>) -> Expr {
    build_multiplicative_expr(pair.into_inner(), false)
}

/// Parse `list_addpart = { mulpart ~ ((mul_op | mod_op) ~ mulpart)* }`
fn parse_list_addpart(pair: Pair<Rule>) -> Expr {
    build_multiplicative_expr(pair.into_inner(), false)
}

/// Shared builder for multiplicative-level expressions (left-associative).
fn build_multiplicative_expr(mut pairs: Pairs<Rule>, _allow_div: bool) -> Expr {
    let first = pairs.next().unwrap();
    let mut left = parse_mulpart(first);
    while let Some(op_pair) = pairs.next() {
        let right_pair = pairs.next().unwrap();
        let right = parse_mulpart(right_pair);
        left = match op_pair.as_rule() {
            Rule::mul_op => Expr::Mul(Box::new(left), Box::new(right)),
            Rule::div_op => Expr::Div(Box::new(left), Box::new(right)),
            Rule::mod_op => Expr::Mod(Box::new(left), Box::new(right)),
            _ => unreachable!(
                "expected mul_op/div_op/mod_op, got {:?}",
                op_pair.as_rule()
            ),
        };
    }
    left
}

/// Parse `mulpart = { minusexpr | DESIRED_NUMBER | NUMBER | inconsistent_varspec | extvarname | bracketexpr }`
fn parse_mulpart(pair: Pair<Rule>) -> Expr {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::minusexpr => {
            let child = inner.into_inner().next().unwrap();
            Expr::Neg(Box::new(parse_mulpart(child)))
        }
        Rule::DESIRED_NUMBER => {
            // Strip trailing '?'
            let s = inner.as_str();
            let num_str = &s[..s.len() - 1];
            Expr::DesiredNumber(num_str.parse::<f64>().unwrap())
        }
        Rule::NUMBER => Expr::Number(inner.as_str().parse::<f64>().unwrap()),
        Rule::inconsistent_varspec => {
            let evn = parse_extvarname(inner.into_inner().next().unwrap());
            Expr::InconsistentVar(evn)
        }
        Rule::extvarname => Expr::Variable(parse_extvarname(inner)),
        Rule::bracketexpr => {
            let expr_pair = inner.into_inner().next().unwrap();
            Expr::Bracket(Box::new(parse_expr(expr_pair)))
        }
        _ => unreachable!("unexpected mulpart variant: {:?}", inner.as_rule()),
    }
}

// ---------------------------------------------------------------------------
// Boolean expressions
// ---------------------------------------------------------------------------

fn parse_bool_expr(pair: Pair<Rule>) -> BoolExpr {
    // pair is `disjunction` or `if_head` wrapping a `disjunction`
    match pair.as_rule() {
        Rule::if_head => {
            let inner = pair.into_inner().next().unwrap();
            parse_disjunction(inner)
        }
        Rule::disjunction => parse_disjunction(pair),
        _ => unreachable!("expected if_head or disjunction, got {:?}", pair.as_rule()),
    }
}

fn parse_disjunction(pair: Pair<Rule>) -> BoolExpr {
    let mut inner = pair.into_inner();
    let mut left = parse_conjunction(inner.next().unwrap());
    for conj in inner {
        let right = parse_conjunction(conj);
        left = BoolExpr::Or(Box::new(left), Box::new(right));
    }
    left
}

fn parse_conjunction(pair: Pair<Rule>) -> BoolExpr {
    let mut inner = pair.into_inner();
    let mut left = parse_comparison(inner.next().unwrap());
    for comp in inner {
        let right = parse_comparison(comp);
        left = BoolExpr::And(Box::new(left), Box::new(right));
    }
    left
}

fn parse_comparison(pair: Pair<Rule>) -> BoolExpr {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::if_condition => {
            let mut cond_inner = inner.into_inner();
            let left = parse_expr(cond_inner.next().unwrap());
            let op_str = cond_inner.next().unwrap().as_str();
            let op = match op_str {
                "==" => CmpOp::Eq,
                "!=" => CmpOp::Ne,
                "<" => CmpOp::Lt,
                ">" => CmpOp::Gt,
                "<=" => CmpOp::Le,
                ">=" => CmpOp::Ge,
                _ => unreachable!("unknown comparison operator: {}", op_str),
            };
            let right = parse_expr(cond_inner.next().unwrap());
            BoolExpr::Comparison {
                left: Box::new(left),
                op,
                right: Box::new(right),
            }
        }
        Rule::disjunction => parse_disjunction(inner),
        _ => unreachable!("unexpected comparison variant: {:?}", inner.as_rule()),
    }
}

// ---------------------------------------------------------------------------
// Loops, conditionals, sections, abbreviations
// ---------------------------------------------------------------------------

fn parse_for_loop(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let for_head = inner.next().unwrap();
    let mut fh = for_head.into_inner();
    let var = fh.next().unwrap().as_str().to_string();
    let start = Box::new(parse_expr(
        fh.next().unwrap().into_inner().next().unwrap(),
    ));
    let stop = Box::new(parse_expr(
        fh.next().unwrap().into_inner().next().unwrap(),
    ));
    let for_body = inner.next().unwrap();
    let body = parse_body(for_body.into_inner());
    RecipeNode::ForLoop {
        var,
        start,
        stop,
        body,
    }
}

fn parse_repeat_loop(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let repeat_head = inner.next().unwrap();
    let var = repeat_head
        .into_inner()
        .find(|p| p.as_rule() == Rule::repeat_varassign)
        .map(|p| {
            let mut vi = p.into_inner();
            let name = vi.next().unwrap().as_str().to_string();
            let init = Box::new(parse_expr(vi.next().unwrap()));
            (name, init)
        });
    let repeat_body = inner.next().unwrap();
    let body = parse_body(repeat_body.into_inner());
    let repeat_tail = inner.next().unwrap();
    let until_head = repeat_tail.into_inner().next().unwrap();
    let until = parse_bool_expr(until_head);
    RecipeNode::RepeatLoop { var, body, until }
}

fn parse_if_clause(pair: Pair<Rule>) -> RecipeNode {
    let mut branches = Vec::new();
    let mut else_body = None;

    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::if_statement | Rule::elif_statement => {
                let mut inner = p.into_inner();
                let head = inner.next().unwrap(); // if_head
                let condition = parse_bool_expr(head);
                let mut lookahead = None;
                let mut body = Vec::new();
                for sub in inner {
                    match sub.as_rule() {
                        Rule::lookahead_option => {
                            let la_expr =
                                sub.into_inner().next().unwrap(); // expr
                            lookahead = Some(Box::new(parse_expr(la_expr)));
                        }
                        Rule::if_body => {
                            body = parse_body(sub.into_inner());
                        }
                        _ => {}
                    }
                }
                branches.push(IfBranch {
                    condition,
                    lookahead,
                    body,
                });
            }
            Rule::else_statement => {
                let body_pair = p.into_inner().next().unwrap();
                else_body = Some(parse_body(body_pair.into_inner()));
            }
            _ => {} // "endif" keyword
        }
    }

    RecipeNode::IfClause {
        branches,
        else_body,
    }
}

fn parse_section(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let head = inner.next().unwrap(); // section_head
    let name = parse_extvarname(
        head.into_inner()
            .find(|p| p.as_rule() == Rule::extvarname)
            .unwrap(),
    );
    let body_pair = inner.next().unwrap(); // section_body
    let body = parse_body(body_pair.into_inner());
    // section_tail is consumed but we don't need its data
    RecipeNode::Section { name, body }
}

fn parse_abbreviation(pair: Pair<Rule>) -> RecipeNode {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let expr = Box::new(parse_expr(inner.next().unwrap()));
    RecipeNode::Abbreviation { name, expr }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tapehead() {
        let recipe = "[MAT, 0, 0/ TAPEDESCR]TEXT\nSEND\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 2);
        match &nodes[0] {
            RecipeNode::Text { ctrl, placeholders } => {
                assert!(matches!(ctrl.mat, CtrlField::Symbolic));
                assert!(matches!(ctrl.mf, CtrlField::Literal(0)));
                assert!(matches!(ctrl.mt, CtrlField::Literal(0)));
                assert_eq!(placeholders.len(), 1);
                assert_eq!(
                    placeholders[0].var.as_ref().unwrap().name,
                    "TAPEDESCR"
                );
            }
            other => panic!("expected Text, got {:?}", other),
        }
        assert!(matches!(nodes[1], RecipeNode::Send));
    }

    #[test]
    fn test_parse_head_tab1_send() {
        let recipe = "\
[MAT, 3, MT/ ZA, AWR, 0, 0, 0, 0]HEAD\n\
[MAT, 3, MT/ QM, QI, 0, LR, NR, NP / E / xs]TAB1\n\
SEND\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 3);
        match &nodes[0] {
            RecipeNode::HeadOrCont { subtype, .. } => {
                assert_eq!(*subtype, ContSubtype::Head);
            }
            other => panic!("expected HeadOrCont, got {:?}", other),
        }
        match &nodes[1] {
            RecipeNode::Tab1 {
                x_var,
                y_var,
                table_name,
                ..
            } => {
                assert_eq!(x_var.name, "E");
                assert_eq!(y_var.name, "xs");
                assert!(table_name.is_none());
            }
            other => panic!("expected Tab1, got {:?}", other),
        }
        assert!(matches!(nodes[2], RecipeNode::Send));
    }

    #[test]
    fn test_parse_for_loop() {
        let recipe = "\
for i = 1 to NR:\n\
[MAT, 3, MT/ C1, C2, L1, L2, N1, N2]CONT\n\
endfor\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            RecipeNode::ForLoop {
                var,
                body,
                start,
                stop,
            } => {
                assert_eq!(var, "i");
                assert_eq!(body.len(), 1);
                match start.as_ref() {
                    Expr::Number(n) => assert_eq!(*n, 1.0),
                    other => panic!("expected Number, got {:?}", other),
                }
                match stop.as_ref() {
                    Expr::Variable(v) => assert_eq!(v.name, "NR"),
                    other => panic!("expected Variable, got {:?}", other),
                }
            }
            other => panic!("expected ForLoop, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_if_elif_else() {
        let recipe = "\
if LFW == 0:\n\
[MAT, 2, MT/ C1, C2, L1, L2, N1, N2]CONT\n\
elif LFW == 1:\n\
SEND\n\
else:\n\
SEND\n\
endif\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            RecipeNode::IfClause {
                branches,
                else_body,
            } => {
                assert_eq!(branches.len(), 2);
                assert!(else_body.is_some());
                assert_eq!(else_body.as_ref().unwrap().len(), 1);
            }
            other => panic!("expected IfClause, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nested_sections() {
        let recipe = "\
(outer)\n\
(inner)\n\
SEND\n\
(/inner)\n\
(/outer)\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            RecipeNode::Section { name, body } => {
                assert_eq!(name.name, "outer");
                assert_eq!(body.len(), 1);
                match &body[0] {
                    RecipeNode::Section {
                        name: inner_name,
                        body: inner_body,
                    } => {
                        assert_eq!(inner_name.name, "inner");
                        assert_eq!(inner_body.len(), 1);
                        assert!(matches!(inner_body[0], RecipeNode::Send));
                    }
                    other => panic!("expected inner Section, got {:?}", other),
                }
            }
            other => panic!("expected Section, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_arithmetic_expression() {
        // Test via abbreviation: VARNAME := expr
        let recipe = "result := 6*NRS+(MPAR*NRS)*(MPAR*NRS+1)/2\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            RecipeNode::Abbreviation { name, expr } => {
                assert_eq!(name, "result");
                // The expression should be: Add(Mul(6, NRS), Div(Mul(Bracket(Mul(MPAR,NRS)), Bracket(Add(Mul(MPAR,NRS), 1))), 2))
                // Just verify it parsed without error and has the right top-level shape
                match expr.as_ref() {
                    Expr::Add(left, right) => {
                        // left = 6*NRS
                        match left.as_ref() {
                            Expr::Mul(_, _) => {}
                            other => panic!("expected Mul for left of Add, got {:?}", other),
                        }
                        // right = (MPAR*NRS)*(MPAR*NRS+1)/2
                        match right.as_ref() {
                            Expr::Div(_, denom) => {
                                match denom.as_ref() {
                                    Expr::Number(n) => assert_eq!(*n, 2.0),
                                    other => panic!("expected Number(2) as denominator, got {:?}", other),
                                }
                            }
                            other => panic!("expected Div for right of Add, got {:?}", other),
                        }
                    }
                    other => panic!("expected Add at top level, got {:?}", other),
                }
            }
            other => panic!("expected Abbreviation, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_tab1_with_table_name() {
        let recipe = "\
[MAT, 3, MT/ QM, QI, 0, LR, NR, NP / E / xs]TAB1(mytable)\n\
SEND\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 2);
        match &nodes[0] {
            RecipeNode::Tab1 { table_name, .. } => {
                assert!(table_name.is_some());
                assert_eq!(table_name.as_ref().unwrap().name, "mytable");
            }
            other => panic!("expected Tab1, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_stop_line() {
        let recipe = "[MAT, 0, 0/ TAPEDESCR]TEXT\nstop(\"something went wrong\")\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 2);
        match &nodes[1] {
            RecipeNode::Stop { message } => {
                assert_eq!(message.as_deref(), Some("something went wrong"));
            }
            other => panic!("expected Stop, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_bool_or_and() {
        let recipe = "\
if A == 1 or B == 2 and C == 3:\n\
SEND\n\
endif\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            RecipeNode::IfClause { branches, .. } => {
                assert_eq!(branches.len(), 1);
                // Should be Or(Comparison(A==1), And(Comparison(B==2), Comparison(C==3)))
                match &branches[0].condition {
                    BoolExpr::Or(_, _) => {}
                    other => panic!("expected Or at top level, got {:?}", other),
                }
            }
            other => panic!("expected IfClause, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_list_record() {
        let recipe = "\
[MAT, 2, MT/ SPI, AP, 0, 0, NLS, 0 / AWRI, QX, L, LRX, 6*NRS+N, NRS,\n\
{ER, AJ, GT, GN, GG, GF}{j=1 to NRS}\n\
]LIST\n";
        let nodes = parse_recipe(recipe).unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            RecipeNode::List { body, .. } => {
                // Should have: AWRI, QX, L, LRX, 6*NRS+N, NRS, then a loop
                // Count the Value items and Loop items
                let value_count = body.iter().filter(|i| matches!(i, ListItem::Value(_))).count();
                let loop_count = body.iter().filter(|i| matches!(i, ListItem::Loop { .. })).count();
                assert_eq!(value_count, 6, "expected 6 value items in list body");
                assert_eq!(loop_count, 1, "expected 1 loop in list body");
            }
            other => panic!("expected List, got {:?}", other),
        }
    }
}
