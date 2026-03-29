use indexmap::IndexMap;
use rustc_hash::FxHasher;
use serde::{Serialize, Deserialize, Serializer, Deserializer, de};
use std::fmt;
use std::hash::BuildHasherDefault;

/// Fast IndexMap using FxHash instead of SipHash.
/// Safe for trusted data (ENDF keys are not adversarial).
pub type FxBuildHasher = BuildHasherDefault<FxHasher>;
pub type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;

/// Key type for ENDF dictionaries. Can be integer (array indices, MF/MT numbers) or string (variable names).
///
/// Serializes to JSON as a string: integers become `"i:42"`, strings become `"s:name"`.
/// This encoding ensures lossless roundtrip through JSON (which only supports string keys).
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum EndfKey {
    Int(i64),
    Str(String),
}

impl Serialize for EndfKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            EndfKey::Int(v) => serializer.serialize_str(&format!("i:{}", v)),
            EndfKey::Str(v) => serializer.serialize_str(&format!("s:{}", v)),
        }
    }
}

impl<'de> Deserialize<'de> for EndfKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        if let Some(rest) = s.strip_prefix("i:") {
            rest.parse::<i64>()
                .map(EndfKey::Int)
                .map_err(de::Error::custom)
        } else if let Some(rest) = s.strip_prefix("s:") {
            Ok(EndfKey::Str(rest.to_string()))
        } else {
            Err(de::Error::custom(format!("invalid EndfKey encoding: '{}'", s)))
        }
    }
}

impl From<i64> for EndfKey {
    fn from(v: i64) -> Self { EndfKey::Int(v) }
}

impl From<i32> for EndfKey {
    fn from(v: i32) -> Self { EndfKey::Int(v as i64) }
}

impl From<&str> for EndfKey {
    fn from(v: &str) -> Self { EndfKey::Str(v.to_string()) }
}

impl From<String> for EndfKey {
    fn from(v: String) -> Self { EndfKey::Str(v) }
}

impl fmt::Display for EndfKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndfKey::Int(v) => write!(f, "{}", v),
            EndfKey::Str(v) => write!(f, "{}", v),
        }
    }
}

/// Table data from TAB1/TAB2 records.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EndfTable {
    pub nbt: Vec<i64>,
    pub int: Vec<i64>,
    pub x: Vec<f64>,   // TAB1 only (empty for TAB2)
    pub y: Vec<f64>,   // TAB1 only (empty for TAB2)
}

impl EndfTable {
    pub fn new_tab1(nbt: Vec<i64>, int: Vec<i64>, x: Vec<f64>, y: Vec<f64>) -> Self {
        Self { nbt, int, x, y }
    }

    pub fn new_tab2(nbt: Vec<i64>, int: Vec<i64>) -> Self {
        Self { nbt, int, x: Vec::new(), y: Vec::new() }
    }

    pub fn is_tab1(&self) -> bool {
        !self.x.is_empty() || !self.y.is_empty()
    }
}

/// The core dynamic value type for ENDF data.
///
/// ENDF data is inherently dynamically typed: the same structure can contain
/// integers, floats, strings, nested dictionaries (sections), lists (arrays),
/// and interpolation tables.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EndfValue {
    /// Integer value (control fields, counters, flags)
    Int(i64),
    /// Floating-point value
    Float(f64),
    /// String value (TEXT records, variable names)
    Str(String),
    /// Ordered dictionary (sections, MF/MT containers, variable collections)
    Dict(FxIndexMap<EndfKey, EndfValue>),
    /// List-mode array (dense, with None for gaps)
    List(Vec<Option<EndfValue>>),
    /// Interpolation table data (TAB1/TAB2)
    Table(EndfTable),
}

impl EndfValue {
    /// Create an empty dictionary.
    pub fn new_dict() -> Self {
        EndfValue::Dict(FxIndexMap::default())
    }

    /// Create an empty list.
    pub fn new_list() -> Self {
        EndfValue::List(Vec::new())
    }

    /// Try to get this value as an i64.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            EndfValue::Int(v) => Some(*v),
            EndfValue::Float(v) if *v == (*v as i64) as f64 => Some(*v as i64),
            _ => None,
        }
    }

    /// Try to get this value as an f64.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            EndfValue::Float(v) => Some(*v),
            EndfValue::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Try to get this value as a string slice.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            EndfValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get this value as a dictionary.
    pub fn as_dict(&self) -> Option<&FxIndexMap<EndfKey, EndfValue>> {
        match self {
            EndfValue::Dict(d) => Some(d),
            _ => None,
        }
    }

    /// Try to get this value as a mutable dictionary.
    pub fn as_dict_mut(&mut self) -> Option<&mut FxIndexMap<EndfKey, EndfValue>> {
        match self {
            EndfValue::Dict(d) => Some(d),
            _ => None,
        }
    }

    /// Try to get this value as a list.
    pub fn as_list(&self) -> Option<&Vec<Option<EndfValue>>> {
        match self {
            EndfValue::List(l) => Some(l),
            _ => None,
        }
    }

    /// Try to get this value as a mutable list.
    pub fn as_list_mut(&mut self) -> Option<&mut Vec<Option<EndfValue>>> {
        match self {
            EndfValue::List(l) => Some(l),
            _ => None,
        }
    }

    /// Try to get this value as a table.
    pub fn as_table(&self) -> Option<&EndfTable> {
        match self {
            EndfValue::Table(t) => Some(t),
            _ => None,
        }
    }

    /// Check if this is a dictionary.
    pub fn is_dict(&self) -> bool {
        matches!(self, EndfValue::Dict(_))
    }

    /// Check if this is a list.
    pub fn is_list(&self) -> bool {
        matches!(self, EndfValue::List(_))
    }

    /// Get a value from a dictionary by key.
    pub fn get<K: Into<EndfKey>>(&self, key: K) -> Option<&EndfValue> {
        self.as_dict()?.get(&key.into())
    }

    /// Get a mutable value from a dictionary by key.
    pub fn get_mut<K: Into<EndfKey>>(&mut self, key: K) -> Option<&mut EndfValue> {
        self.as_dict_mut()?.get_mut(&key.into())
    }

    /// Insert a value into a dictionary.
    pub fn insert<K: Into<EndfKey>>(&mut self, key: K, value: EndfValue) {
        if let EndfValue::Dict(d) = self {
            d.insert(key.into(), value);
        }
    }

    /// Check if a dictionary contains a key.
    pub fn contains_key<K: Into<EndfKey>>(&self, key: K) -> bool {
        self.as_dict().map_or(false, |d| d.contains_key(&key.into()))
    }

    /// Get a value by path (e.g., "3/1/QM" for mf=3, mt=1, variable QM).
    /// Path segments that parse as i64 are treated as integer keys.
    pub fn get_path(&self, path: &str) -> Option<&EndfValue> {
        let mut current = self;
        for segment in path.split('/') {
            let key = if let Ok(i) = segment.parse::<i64>() {
                EndfKey::Int(i)
            } else {
                EndfKey::Str(segment.to_string())
            };
            current = current.get(key)?;
        }
        Some(current)
    }

    /// Set a value by path, creating intermediate dictionaries as needed.
    pub fn set_path(&mut self, path: &str, value: EndfValue) {
        let segments: Vec<&str> = path.split('/').collect();
        let mut current = self;
        for (i, segment) in segments.iter().enumerate() {
            let key: EndfKey = if let Ok(n) = segment.parse::<i64>() {
                EndfKey::Int(n)
            } else {
                EndfKey::Str(segment.to_string())
            };
            if i == segments.len() - 1 {
                current.insert(key, value);
                return;
            }
            // Ensure intermediate dict exists
            if !current.contains_key(key.clone()) {
                current.insert(key.clone(), EndfValue::new_dict());
            }
            current = current.get_mut(key).unwrap();
        }
    }
}

impl fmt::Display for EndfValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndfValue::Int(v) => write!(f, "{}", v),
            EndfValue::Float(v) => write!(f, "{}", v),
            EndfValue::Str(s) => write!(f, "\"{}\"", s),
            EndfValue::Dict(d) => write!(f, "{{dict with {} entries}}", d.len()),
            EndfValue::List(l) => write!(f, "[list with {} entries]", l.len()),
            EndfValue::Table(t) => {
                if t.is_tab1() {
                    write!(f, "Table(NR={}, NP={})", t.nbt.len(), t.x.len())
                } else {
                    write!(f, "Table(NR={})", t.nbt.len())
                }
            }
        }
    }
}

impl From<i64> for EndfValue {
    fn from(v: i64) -> Self { EndfValue::Int(v) }
}

impl From<i32> for EndfValue {
    fn from(v: i32) -> Self { EndfValue::Int(v as i64) }
}

impl From<f64> for EndfValue {
    fn from(v: f64) -> Self { EndfValue::Float(v) }
}

impl From<String> for EndfValue {
    fn from(v: String) -> Self { EndfValue::Str(v) }
}

impl From<&str> for EndfValue {
    fn from(v: &str) -> Self { EndfValue::Str(v.to_string()) }
}
