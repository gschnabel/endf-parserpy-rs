/// A complete parsed recipe (sequence of nodes).
pub type Recipe = Vec<RecipeNode>;

/// A single instruction in a recipe.
#[derive(Clone, Debug)]
pub enum RecipeNode {
    /// HEAD or CONT record: [MAT,MF,MT/ expr,expr,expr,expr,expr,expr] HEAD/CONT
    HeadOrCont {
        ctrl: CtrlSpec,
        fields: [Expr; 6],
        subtype: ContSubtype,
    },
    /// TEXT record: [MAT,MF,MT/ textfield1{len}, textfield2{len}] TEXT
    Text {
        ctrl: CtrlSpec,
        placeholders: Vec<TextPlaceholder>,
    },
    /// DIR record: [MAT,MF,MT/ blank,blank, expr,expr,expr,expr] DIR
    Dir {
        ctrl: CtrlSpec,
        fields: [Expr; 4],
    },
    /// INTG record: [MAT,MF,MT/ expr,expr,expr {ndigit}] INTG
    Intg {
        ctrl: CtrlSpec,
        fields: [Expr; 3],
        ndigit: Box<Expr>,
    },
    /// TAB1 record
    Tab1 {
        ctrl: CtrlSpec,
        fields: [Expr; 6],
        x_var: ExtVarName,
        y_var: ExtVarName,
        table_name: Option<ExtVarName>,
    },
    /// TAB2 record
    Tab2 {
        ctrl: CtrlSpec,
        fields: [Expr; 6],
        z_var: ExtVarName,
        table_name: Option<ExtVarName>,
    },
    /// LIST record
    List {
        ctrl: CtrlSpec,
        fields: [Expr; 6],
        body: Vec<ListItem>,
        list_name: Option<ExtVarName>,
    },
    /// SEND record
    Send,
    /// stop("message")
    Stop { message: Option<String> },
    /// for VAR = start to stop: body endfor
    ForLoop {
        var: String,
        start: Box<Expr>,
        stop: Box<Expr>,
        body: Vec<RecipeNode>,
    },
    /// repeat [VAR = init]: body until condition
    RepeatLoop {
        var: Option<(String, Box<Expr>)>,
        body: Vec<RecipeNode>,
        until: BoolExpr,
    },
    /// if/elif/else/endif
    IfClause {
        branches: Vec<IfBranch>,
        else_body: Option<Vec<RecipeNode>>,
    },
    /// (section_name) ... (/section_name)
    Section {
        name: ExtVarName,
        body: Vec<RecipeNode>,
    },
    /// VARNAME := expr
    Abbreviation {
        name: String,
        expr: Box<Expr>,
    },
    /// Comment block (preserved for documentation extraction)
    Comment(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ContSubtype {
    Head,
    Cont,
}

/// An if/elif branch with condition, optional lookahead, and body.
#[derive(Clone, Debug)]
pub struct IfBranch {
    pub condition: BoolExpr,
    pub lookahead: Option<Box<Expr>>,
    pub body: Vec<RecipeNode>,
}

/// Control specification: MAT, MF, MT (each symbolic or literal).
#[derive(Clone, Debug)]
pub struct CtrlSpec {
    pub mat: CtrlField,
    pub mf: CtrlField,
    pub mt: CtrlField,
}

/// A control field can be symbolic (e.g., "MAT") or a literal integer.
#[derive(Clone, Debug)]
pub enum CtrlField {
    Symbolic,
    Literal(i32),
}

/// A text placeholder in TEXT records.
#[derive(Clone, Debug)]
pub struct TextPlaceholder {
    pub var: Option<ExtVarName>,
    pub width: Option<usize>,
}

/// An item in a LIST record body.
#[derive(Clone, Debug)]
pub enum ListItem {
    /// A single expression value
    Value(Expr),
    /// A loop: {body}{var=start to stop}
    Loop {
        body: Vec<ListItem>,
        var: String,
        start: Box<Expr>,
        stop: Box<Expr>,
    },
    /// PADLINE — pad to 6-element boundary
    Padding,
}

/// Arithmetic expression.
#[derive(Clone, Debug)]
pub enum Expr {
    /// Literal number
    Number(f64),
    /// Number with ? suffix (expected but may differ in practice)
    DesiredNumber(f64),
    /// Variable reference (possibly indexed)
    Variable(ExtVarName),
    /// Variable with ? suffix (value allowed to be inconsistent)
    InconsistentVar(ExtVarName),
    /// Unary negation
    Neg(Box<Expr>),
    /// Binary addition
    Add(Box<Expr>, Box<Expr>),
    /// Binary subtraction
    Sub(Box<Expr>, Box<Expr>),
    /// Binary multiplication
    Mul(Box<Expr>, Box<Expr>),
    /// Binary division
    Div(Box<Expr>, Box<Expr>),
    /// Binary modulo
    Mod(Box<Expr>, Box<Expr>),
    /// Parenthesized expression
    Bracket(Box<Expr>),
}

/// Extended variable name with optional array indices.
#[derive(Clone, Debug)]
pub struct ExtVarName {
    pub name: String,
    pub indices: Vec<Expr>,
}

impl ExtVarName {
    pub fn simple(name: &str) -> Self {
        Self { name: name.to_string(), indices: Vec::new() }
    }

    pub fn with_indices(name: &str, indices: Vec<Expr>) -> Self {
        Self { name: name.to_string(), indices }
    }

    pub fn is_scalar(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Boolean expression (used in if conditions and repeat-until).
#[derive(Clone, Debug)]
pub enum BoolExpr {
    /// Comparison: expr op expr
    Comparison {
        left: Box<Expr>,
        op: CmpOp,
        right: Box<Expr>,
    },
    /// Logical AND
    And(Box<BoolExpr>, Box<BoolExpr>),
    /// Logical OR
    Or(Box<BoolExpr>, Box<BoolExpr>),
}

/// Comparison operators.
#[derive(Clone, Debug, PartialEq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}
