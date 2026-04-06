use thiserror::Error;

#[derive(Error, Debug)]
pub enum EndfError {
    #[error("invalid float: '{input}'")]
    InvalidFloat { input: String },
    #[error("invalid integer: '{input}'")]
    InvalidInteger { input: String },
    #[error("number mismatch in field '{field}': expected {expected}, got {got}")]
    NumberMismatch { expected: f64, got: f64, field: String },
    #[error("zero mismatch in field '{field}': expected 0, got {got}")]
    ZeroMismatch { got: f64, field: String },
    #[error("unexpected control record: expected ({expected_mat},{expected_mf},{expected_mt}), got ({got_mat},{got_mf},{got_mt})")]
    UnexpectedControlRecord {
        expected_mat: i32,
        expected_mf: i32,
        expected_mt: i32,
        got_mat: i32,
        got_mf: i32,
        got_mt: i32,
    },
    #[error("unexpected control record: {message}")]
    UnexpectedControlRecordMsg { message: String },
    #[error("unexpected end of input at line {line}")]
    UnexpectedEndOfInput { line: usize },
    #[error("unexpected end of input: {message}")]
    UnexpectedEndOfInputMsg { message: String },
    #[error("blank line at line {line}")]
    BlankLine { line: usize },
    #[error("not a section end record at line {line}")]
    NotSectionEnd { line: usize },
    #[error("not a section end record: {message}")]
    NotSectionEndMsg { message: String },
    #[error("variable not found: '{name}'")]
    VariableNotFound { name: String },
    #[error("index not found: '{name}' with indices {indices:?}")]
    IndexNotFound { name: String, indices: Vec<i64> },
    #[error("several unbound variables in expression")]
    SeveralUnboundVariables,
    #[error("size mismatch: expected {expected}, got {got}")]
    SizeMismatch { expected: usize, got: usize },
    #[error("missing section: '{name}'")]
    MissingSection { name: String },
    #[error("inconsistent variable assignment for '{name}'")]
    InconsistentVariableAssignment { name: String },
    #[error("recipe parse error: {message}")]
    RecipeParse { message: String },
    #[error("variable in denominator: '{name}'")]
    VariableInDenominator { name: String },
    #[error("abbreviation name collision: '{name}'")]
    AbbreviationNameCollision { name: String },
    #[error("more list elements expected: got {got}, expected {expected}")]
    MoreListElementsExpected { expected: usize, got: usize },
    #[error("unconsumed list elements: {remaining} remaining")]
    UnconsumedListElements { remaining: usize },
    #[error("inconsistent section brackets")]
    InconsistentSectionBrackets,
    #[error("loop variable error: '{name}'")]
    LoopVariableError { name: String },
    #[error("stop: {message}")]
    Stop { message: String },
    #[error("non-integer float {value} in integer field '{field}'")]
    NonIntegerField { field: String, value: f64 },
    #[error("float value {value} in integer field '{field}' (strict_datatypes mode)")]
    StrictFloatInIntField { field: String, value: f64 },
    #[error("file already exists: {path}")]
    FileExists { path: std::path::PathBuf },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type EndfResult<T> = Result<T, EndfError>;
