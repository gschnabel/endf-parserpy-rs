use wasm_bindgen::prelude::*;

use endf_parser::parser::EndfParser;
use endf_parser::value::{EndfKey, EndfValue};

/// Convert an EndfValue tree to a JsValue.
fn endf_value_to_js(val: &EndfValue) -> JsValue {
    match val {
        EndfValue::Int(n) => JsValue::from_f64(*n as f64),
        EndfValue::Float(f) => JsValue::from_f64(*f),
        EndfValue::Str(s) => JsValue::from_str(s),
        EndfValue::Dict(map) => {
            let obj = js_sys::Object::new();
            for (key, value) in map {
                let js_key = match key {
                    EndfKey::Int(n) => JsValue::from_str(&n.to_string()),
                    EndfKey::Str(s) => JsValue::from_str(s),
                };
                js_sys::Reflect::set(&obj, &js_key, &endf_value_to_js(value)).unwrap();
            }
            obj.into()
        }
        EndfValue::List(items) => {
            let arr = js_sys::Array::new();
            for item in items {
                match item {
                    Some(v) => arr.push(&endf_value_to_js(v)),
                    None => arr.push(&JsValue::NULL),
                };
            }
            arr.into()
        }
        EndfValue::Table(table) => {
            let obj = js_sys::Object::new();
            let to_js_arr = |v: &[f64]| -> JsValue {
                let arr = js_sys::Array::new();
                for x in v { arr.push(&JsValue::from_f64(*x)); }
                arr.into()
            };
            let to_js_int_arr = |v: &[i64]| -> JsValue {
                let arr = js_sys::Array::new();
                for x in v { arr.push(&JsValue::from_f64(*x as f64)); }
                arr.into()
            };
            js_sys::Reflect::set(&obj, &"NBT".into(), &to_js_int_arr(&table.nbt)).unwrap();
            js_sys::Reflect::set(&obj, &"INT".into(), &to_js_int_arr(&table.int)).unwrap();
            if !table.x.is_empty() {
                js_sys::Reflect::set(&obj, &"x".into(), &to_js_arr(&table.x)).unwrap();
                js_sys::Reflect::set(&obj, &"y".into(), &to_js_arr(&table.y)).unwrap();
            }
            obj.into()
        }
    }
}

/// A WASM-accessible ENDF parser.
#[wasm_bindgen]
pub struct WasmEndfParser {
    inner: EndfParser,
}

#[wasm_bindgen]
impl WasmEndfParser {
    /// Create a new parser with the given ENDF format.
    ///
    /// Supported formats: "endf6", "endf6-ext", "jendl", "pendf", "errorr"
    #[wasm_bindgen(constructor)]
    pub fn new(endf_format: Option<String>) -> Result<WasmEndfParser, JsError> {
        let fmt = endf_format.as_deref().unwrap_or("endf6");
        let inner = EndfParser::builder()
            .endf_format(fmt)
            .ignore_number_mismatch(true)
            .ignore_zero_mismatch(true)
            .ignore_varspec_mismatch(true)
            .ignore_send_records(true)
            .ignore_missing_tpid(true)
            .ignore_blank_lines(true)
            .build()
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(WasmEndfParser { inner })
    }

    /// Parse ENDF text and return a JavaScript object.
    ///
    /// The returned object has the structure: { mf: { mt: { field: value, ... } } }
    pub fn parse(&self, input: &str) -> Result<JsValue, JsError> {
        let data = self.inner.parse(input)
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(endf_value_to_js(&data))
    }

    /// Write a JavaScript object back to ENDF text.
    ///
    /// `opts` is an optional JS object with boolean fields:
    /// `keep_E`, `abuse_signpos`, `skip_intzero`, `prefer_noexp`
    pub fn write(&self, data: JsValue, opts: JsValue) -> Result<String, JsError> {
        let endf_data = js_to_endf_value(data)
            .map_err(|e| JsError::new(&e.to_string()))?;

        if opts.is_object() && !opts.is_null() && !opts.is_undefined() {
            let mut builder = EndfParser::builder();
            // Copy parse/read settings from self
            builder = builder.endf_format("endf6")
                .ignore_number_mismatch(true)
                .ignore_zero_mismatch(true)
                .ignore_varspec_mismatch(true)
                .ignore_send_records(true)
                .ignore_missing_tpid(true)
                .ignore_blank_lines(true);

            if let Some(v) = js_sys::Reflect::get(&opts, &"keep_E".into()).ok().and_then(|v| v.as_bool()) {
                builder = builder.keep_e(v);
            }
            if let Some(v) = js_sys::Reflect::get(&opts, &"abuse_signpos".into()).ok().and_then(|v| v.as_bool()) {
                builder = builder.abuse_signpos(v);
            }
            if let Some(v) = js_sys::Reflect::get(&opts, &"skip_intzero".into()).ok().and_then(|v| v.as_bool()) {
                builder = builder.skip_intzero(v);
            }
            if let Some(v) = js_sys::Reflect::get(&opts, &"prefer_noexp".into()).ok().and_then(|v| v.as_bool()) {
                builder = builder.prefer_noexp(v);
            }

            let writer = builder.build().map_err(|e| JsError::new(&e.to_string()))?;
            writer.write(&endf_data).map_err(|e| JsError::new(&e.to_string()))
        } else {
            self.inner.write(&endf_data).map_err(|e| JsError::new(&e.to_string()))
        }
    }

    /// Return the list of available ENDF format names.
    pub fn available_formats() -> JsValue {
        let arr = js_sys::Array::new();
        arr.push(&JsValue::from_str("endf6"));
        arr.push(&JsValue::from_str("endf6-ext"));
        arr.push(&JsValue::from_str("jendl"));
        arr.push(&JsValue::from_str("pendf"));
        arr.push(&JsValue::from_str("errorr"));
        arr.into()
    }
}

/// Convert a JsValue back to an EndfValue (for write support).
fn js_to_endf_value(val: JsValue) -> Result<EndfValue, String> {
    if val.is_null() || val.is_undefined() {
        return Ok(EndfValue::Int(0));
    }
    if let Some(n) = val.as_f64() {
        if n.fract() == 0.0 && n.abs() < i64::MAX as f64 {
            return Ok(EndfValue::Int(n as i64));
        }
        return Ok(EndfValue::Float(n));
    }
    if let Some(s) = val.as_string() {
        return Ok(EndfValue::Str(s));
    }
    if js_sys::Array::is_array(&val) {
        let arr = js_sys::Array::from(&val);
        let mut items = Vec::new();
        for i in 0..arr.length() {
            let v = arr.get(i);
            if v.is_null() || v.is_undefined() {
                items.push(None);
            } else {
                items.push(Some(js_to_endf_value(v)?));
            }
        }
        return Ok(EndfValue::List(items));
    }
    if val.is_object() {
        let obj = js_sys::Object::from(val);
        let mut dict = EndfValue::new_dict();
        let entries = js_sys::Object::entries(&obj);
        for i in 0..entries.length() {
            let pair = js_sys::Array::from(&entries.get(i));
            let key_str = pair.get(0).as_string()
                .ok_or_else(|| "non-string key".to_string())?;
            let value = js_to_endf_value(pair.get(1))?;
            let key = if let Ok(n) = key_str.parse::<i64>() {
                EndfKey::Int(n)
            } else {
                EndfKey::Str(key_str)
            };
            dict.insert(key, value);
        }
        return Ok(dict);
    }
    Err("unsupported JS type".to_string())
}
