/// Options controlling how ENDF records are read from text.
#[derive(Clone, Debug)]
pub struct ReadOpts {
    pub width: usize,
    pub accept_spaces: bool,
    pub preserve_value_strings: bool,
    pub ignore_blank_lines: bool,
    pub ignore_send_records: bool,
    pub ignore_missing_tpid: bool,
    pub nofail: bool,
}

impl Default for ReadOpts {
    fn default() -> Self {
        Self {
            width: 11,
            accept_spaces: true,
            preserve_value_strings: false,
            ignore_blank_lines: false,
            ignore_send_records: false,
            ignore_missing_tpid: false,
            nofail: false,
        }
    }
}

/// Options controlling how ENDF records are written to text.
#[derive(Clone, Debug)]
pub struct WriteOpts {
    pub width: usize,
    pub abuse_signpos: bool,
    pub skip_intzero: bool,
    pub prefer_noexp: bool,
    pub keep_e: bool,
    pub include_linenum: bool,
    pub strict_datatypes: bool,
    pub zero_as_blank: bool,
}

impl Default for WriteOpts {
    fn default() -> Self {
        // Defaults match the Python endf-parserpy reference implementation:
        // abuse_signpos / skip_intzero / prefer_noexp are opt-in precision
        // extensions that violate strict ENDF column semantics, so they
        // are disabled by default. Enable them via the builder to get
        // maximum precision at the cost of non-standard layout.
        Self {
            width: 11,
            abuse_signpos: false,
            skip_intzero: false,
            prefer_noexp: false,
            keep_e: false,
            include_linenum: true,
            strict_datatypes: false,
            zero_as_blank: false,
        }
    }
}

/// Options controlling how the recipe interpreter processes data.
#[derive(Clone, Debug)]
pub struct ParseOpts {
    pub ignore_number_mismatch: bool,
    pub ignore_zero_mismatch: bool,
    pub ignore_varspec_mismatch: bool,
    pub fuzzy_matching: bool,
    pub array_type: ArrayType,
}

impl Default for ParseOpts {
    fn default() -> Self {
        Self {
            ignore_number_mismatch: false,
            ignore_zero_mismatch: true,
            ignore_varspec_mismatch: true,
            fuzzy_matching: false,
            array_type: ArrayType::Dict,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArrayType {
    Dict,
    List,
}
