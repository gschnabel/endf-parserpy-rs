use pyo3::prelude::*;

mod py_parser;
mod py_value;

#[pymodule]
fn endf_parser_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<py_parser::EndfParser>()?;
    Ok(())
}
