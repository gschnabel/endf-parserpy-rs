use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use endf_parser::value::{EndfKey, EndfValue, EndfTable};

/// Convert an EndfValue to a Python object.
pub fn endf_value_to_py(py: Python<'_>, val: &EndfValue) -> PyResult<PyObject> {
    match val {
        EndfValue::Int(v) => Ok(v.into_py(py)),
        EndfValue::Float(v) => Ok(v.into_py(py)),
        EndfValue::Str(s) => Ok(s.into_py(py)),
        EndfValue::Dict(d) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in d {
                let py_key = endf_key_to_py(py, k)?;
                let py_val = endf_value_to_py(py, v)?;
                dict.set_item(py_key, py_val)?;
            }
            Ok(dict.into_py(py))
        }
        EndfValue::List(l) => {
            let list = PyList::empty_bound(py);
            for item in l {
                match item {
                    Some(v) => list.append(endf_value_to_py(py, v)?)?,
                    None => list.append(py.None())?,
                }
            }
            Ok(list.into_py(py))
        }
        EndfValue::Table(t) => {
            let dict = PyDict::new_bound(py);
            dict.set_item("NBT", endf_i64_vec_to_py(py, &t.nbt)?)?;
            dict.set_item("INT", endf_i64_vec_to_py(py, &t.int)?)?;
            if t.is_tab1() {
                dict.set_item("X", endf_f64_vec_to_py(py, &t.x)?)?;
                dict.set_item("Y", endf_f64_vec_to_py(py, &t.y)?)?;
            }
            Ok(dict.into_py(py))
        }
    }
}

fn endf_i64_vec_to_py(py: Python<'_>, v: &[i64]) -> PyResult<PyObject> {
    let list = PyList::empty_bound(py);
    for item in v {
        list.append(*item)?;
    }
    Ok(list.into_py(py))
}

fn endf_f64_vec_to_py(py: Python<'_>, v: &[f64]) -> PyResult<PyObject> {
    let list = PyList::empty_bound(py);
    for item in v {
        list.append(*item)?;
    }
    Ok(list.into_py(py))
}

fn endf_key_to_py(py: Python<'_>, key: &EndfKey) -> PyResult<PyObject> {
    match key {
        EndfKey::Int(v) => Ok(v.into_py(py)),
        EndfKey::Str(s) => Ok(s.into_py(py)),
    }
}

/// Convert a Python object to an EndfValue.
pub fn py_to_endf_value(obj: &Bound<'_, PyAny>) -> PyResult<EndfValue> {
    if let Ok(v) = obj.extract::<i64>() {
        Ok(EndfValue::Int(v))
    } else if let Ok(v) = obj.extract::<f64>() {
        Ok(EndfValue::Float(v))
    } else if let Ok(v) = obj.extract::<String>() {
        Ok(EndfValue::Str(v))
    } else if let Ok(dict) = obj.downcast::<PyDict>() {
        // Check if this looks like an EndfTable (has NBT and INT keys)
        let has_nbt = dict.get_item("NBT")?.is_some();
        let has_int = dict.get_item("INT")?.is_some();
        let has_x = dict.get_item("X")?.is_some();
        let has_y = dict.get_item("Y")?.is_some();

        if has_nbt && has_int && has_x && has_y {
            // TAB1 table
            let nbt: Vec<i64> = dict.get_item("NBT")?.unwrap().extract()?;
            let int: Vec<i64> = dict.get_item("INT")?.unwrap().extract()?;
            let x: Vec<f64> = dict.get_item("X")?.unwrap().extract()?;
            let y: Vec<f64> = dict.get_item("Y")?.unwrap().extract()?;
            Ok(EndfValue::Table(EndfTable::new_tab1(nbt, int, x, y)))
        } else if has_nbt && has_int {
            // TAB2 table
            let nbt: Vec<i64> = dict.get_item("NBT")?.unwrap().extract()?;
            let int: Vec<i64> = dict.get_item("INT")?.unwrap().extract()?;
            Ok(EndfValue::Table(EndfTable::new_tab2(nbt, int)))
        } else {
            // Regular dict
            let mut map = indexmap::IndexMap::new();
            for (k, v) in dict.iter() {
                let key = py_to_endf_key(&k)?;
                let val = py_to_endf_value(&v)?;
                map.insert(key, val);
            }
            Ok(EndfValue::Dict(map))
        }
    } else if let Ok(list) = obj.downcast::<PyList>() {
        let mut items = Vec::new();
        for item in list.iter() {
            if item.is_none() {
                items.push(None);
            } else {
                items.push(Some(py_to_endf_value(&item)?));
            }
        }
        Ok(EndfValue::List(items))
    } else if obj.is_none() {
        Ok(EndfValue::Int(0))
    } else {
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            format!("unsupported type: {}", obj.get_type().name()?),
        ))
    }
}

fn py_to_endf_key(obj: &Bound<'_, PyAny>) -> PyResult<EndfKey> {
    if let Ok(v) = obj.extract::<i64>() {
        Ok(EndfKey::Int(v))
    } else if let Ok(v) = obj.extract::<String>() {
        Ok(EndfKey::Str(v))
    } else {
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            format!("dict key must be int or str, got {}", obj.get_type().name()?),
        ))
    }
}
