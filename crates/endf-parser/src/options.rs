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
        Self {
            width: 11,
            abuse_signpos: true,
            skip_intzero: true,
            prefer_noexp: true,
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
