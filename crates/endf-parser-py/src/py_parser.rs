use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;

use endf_parser::parser::EndfParser as RustParser;
use endf_parser::options::ArrayType;
use super::py_value::{endf_value_to_py, py_to_endf_value};

#[pyclass(name = "EndfParser")]
pub struct EndfParser {
    inner: RustParser,
}

#[pymethods]
impl EndfParser {
    #[new]
    #[pyo3(signature = (**kwargs))]
    fn new(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut builder = RustParser::builder();

        if let Some(kw) = kwargs {
            if let Some(v) = kw.get_item("ignore_number_mismatch")? {
                builder = builder.ignore_number_mismatch(v.extract()?);
            }
            if let Some(v) = kw.get_item("ignore_zero_mismatch")? {
                builder = builder.ignore_zero_mismatch(v.extract()?);
            }
            if let Some(v) = kw.get_item("ignore_varspec_mismatch")? {
                builder = builder.ignore_varspec_mismatch(v.extract()?);
            }
            if let Some(v) = kw.get_item("fuzzy_matching")? {
                builder = builder.fuzzy_matching(v.extract()?);
            }
            if let Some(v) = kw.get_item("accept_spaces")? {
                builder = builder.accept_spaces(v.extract()?);
            }
            if let Some(v) = kw.get_item("ignore_blank_lines")? {
                builder = builder.ignore_blank_lines(v.extract()?);
            }
            if let Some(v) = kw.get_item("ignore_send_records")? {
                builder = builder.ignore_send_records(v.extract()?);
            }
            if let Some(v) = kw.get_item("ignore_missing_tpid")? {
                builder = builder.ignore_missing_tpid(v.extract()?);
            }
            if let Some(v) = kw.get_item("preserve_value_strings")? {
                builder = builder.preserve_value_strings(v.extract()?);
            }
            if let Some(v) = kw.get_item("abuse_signpos")? {
                builder = builder.abuse_signpos(v.extract()?);
            }
            if let Some(v) = kw.get_item("skip_intzero")? {
                builder = builder.skip_intzero(v.extract()?);
            }
            if let Some(v) = kw.get_item("prefer_noexp")? {
                builder = builder.prefer_noexp(v.extract()?);
            }
            if let Some(v) = kw.get_item("keep_E")? {
                builder = builder.keep_e(v.extract()?);
            }
            if let Some(v) = kw.get_item("include_linenum")? {
                builder = builder.include_linenum(v.extract()?);
            }
            if let Some(v) = kw.get_item("width")? {
                builder = builder.width(v.extract()?);
            }
            if let Some(v) = kw.get_item("strict_datatypes")? {
                builder = builder.strict_datatypes(v.extract()?);
            }
            if let Some(v) = kw.get_item("zero_as_blank")? {
                builder = builder.zero_as_blank(v.extract()?);
            }
            if let Some(v) = kw.get_item("recipes_dir")? {
                let dir: String = v.extract()?;
                builder = builder.recipes_dir(dir);
            }
            if let Some(v) = kw.get_item("array_type")? {
                let at: String = v.extract()?;
                builder = builder.array_type(match at.as_str() {
                    "list" => ArrayType::List,
                    _ => ArrayType::Dict,
                });
            }
        }

        let inner = builder.build().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
        })?;

        Ok(Self { inner })
    }

    fn parse(&self, py: Python<'_>, input: &str) -> PyResult<PyObject> {
        let data = self.inner.parse(input).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
        })?;
        endf_value_to_py(py, &data)
    }

    fn parsefile(&self, py: Python<'_>, filename: &str) -> PyResult<PyObject> {
        let data = self.inner.parse_file(Path::new(filename)).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
        })?;
        endf_value_to_py(py, &data)
    }

    fn write(&self, endf_dict: &Bound<'_, PyAny>) -> PyResult<String> {
        let data = py_to_endf_value(endf_dict)?;
        self.inner.write(&data).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
        })
    }

    fn writefile(&self, filename: &str, endf_dict: &Bound<'_, PyAny>) -> PyResult<()> {
        let data = py_to_endf_value(endf_dict)?;
        self.inner.write_file(Path::new(filename), &data).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
        })
    }
}
