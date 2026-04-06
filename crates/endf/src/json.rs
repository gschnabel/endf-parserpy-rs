//! Human-friendly JSON serialization for EndfValue.
//!
//! Produces clean JSON without type tags:
//!   - Int → JSON number (integer)
//!   - Float → JSON number (float)
//!   - Str → JSON string
//!   - Dict → JSON object (integer keys become string keys)
//!   - List → JSON array (None entries become null)
//!
//! For deserialization, integer-looking keys are restored as EndfKey::Int.

use crate::value::{EndfKey, EndfValue, FxIndexMap};
use serde_json::{Value as JsonValue, Map as JsonMap, Number as JsonNumber};

/// Convert EndfValue to a human-friendly serde_json::Value.
pub fn endf_to_json(val: &EndfValue) -> JsonValue {
    match val {
        EndfValue::Int(n) => JsonValue::Number(JsonNumber::from(*n)),
        EndfValue::Float(f) | EndfValue::PreservedFloat(f, _) => {
            match JsonNumber::from_f64(*f) {
                Some(n) => JsonValue::Number(n),
                None => JsonValue::Null, // NaN/Inf
            }
        }
        EndfValue::Str(s) => JsonValue::String(s.clone()),
        EndfValue::Dict(d) => {
            let mut map = JsonMap::new();
            for (k, v) in d {
                let key = match k {
                    EndfKey::Int(n) => n.to_string(),
                    EndfKey::Str(s) => s.clone(),
                };
                map.insert(key, endf_to_json(v));
            }
            JsonValue::Object(map)
        }
        EndfValue::List(l) => {
            let arr: Vec<JsonValue> = l.iter().map(|item| {
                match item {
                    Some(v) => endf_to_json(v),
                    None => JsonValue::Null,
                }
            }).collect();
            JsonValue::Array(arr)
        }
    }
}

/// Convert a serde_json::Value back to EndfValue.
///
/// Keys that parse as integers become EndfKey::Int, others become EndfKey::Str.
/// JSON numbers that are exact integers become EndfValue::Int, others Float.
pub fn json_to_endf(val: &JsonValue) -> EndfValue {
    match val {
        JsonValue::Null => EndfValue::Int(0),
        JsonValue::Bool(b) => EndfValue::Int(if *b { 1 } else { 0 }),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                EndfValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                EndfValue::Float(f)
            } else {
                EndfValue::Float(0.0)
            }
        }
        JsonValue::String(s) => EndfValue::Str(s.clone()),
        JsonValue::Array(arr) => {
            let items: Vec<Option<EndfValue>> = arr.iter().map(|v| {
                if v.is_null() { None } else { Some(json_to_endf(v)) }
            }).collect();
            EndfValue::List(items)
        }
        JsonValue::Object(map) => {
            let mut d = FxIndexMap::default();
            for (k, v) in map {
                let key = if let Ok(n) = k.parse::<i64>() {
                    EndfKey::Int(n)
                } else {
                    EndfKey::Str(k.clone())
                };
                d.insert(key, json_to_endf(v));
            }
            EndfValue::Dict(d)
        }
    }
}

/// Serialize EndfValue to a human-friendly JSON string.
pub fn to_json_string(val: &EndfValue, pretty: bool) -> Result<String, serde_json::Error> {
    let jval = endf_to_json(val);
    if pretty {
        serde_json::to_string_pretty(&jval)
    } else {
        serde_json::to_string(&jval)
    }
}

/// Deserialize EndfValue from a human-friendly JSON string.
pub fn from_json_string(s: &str) -> Result<EndfValue, serde_json::Error> {
    let jval: JsonValue = serde_json::from_str(s)?;
    Ok(json_to_endf(&jval))
}
