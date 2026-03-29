use pyo3::prelude::*;

mod py_parser;
mod py_compiled;
mod py_value;

#[pymodule]
fn endf_parser_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<py_parser::EndfParser>()?;
    m.add_class::<py_compiled::CompiledParser>()?;
    Ok(())
}
