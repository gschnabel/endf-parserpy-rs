use std::fmt;

/// A floating-point value that optionally preserves the original ENDF field string.
#[derive(Clone, Debug)]
pub struct EndfFloat {
    value: f64,
    orig_str: Option<String>,
}

impl EndfFloat {
    pub fn new(value: f64, orig_str: Option<String>) -> Self {
        Self { value, orig_str }
    }

    pub fn from_value(value: f64) -> Self {
        Self {
            value,
            orig_str: None,
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn original_string(&self) -> Option<&str> {
        self.orig_str.as_deref()
    }
}

impl From<f64> for EndfFloat {
    fn from(v: f64) -> Self {
        Self::from_value(v)
    }
}

impl From<EndfFloat> for f64 {
    fn from(ef: EndfFloat) -> f64 {
        ef.value
    }
}

impl PartialEq for EndfFloat {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl PartialEq<f64> for EndfFloat {
    fn eq(&self, other: &f64) -> bool {
        self.value == *other
    }
}

impl PartialOrd for EndfFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl PartialOrd<f64> for EndfFloat {
    fn partial_cmp(&self, other: &f64) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(other)
    }
}

impl fmt::Display for EndfFloat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref s) = self.orig_str {
            write!(f, "{}", s)
        } else {
            write!(f, "{}", self.value)
        }
    }
}
