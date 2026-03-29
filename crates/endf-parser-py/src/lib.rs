use pyo3::prelude::*;

mod py_parser;
mod py_compiled;
mod py_value;

#[pymodule]
fn endf_parser_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Configure rayon to use 2 threads (modest parallelism, leave cores
    // for other work). Ignore error if already initialized.
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build_global();

    m.add_class::<py_parser::EndfParser>()?;
    m.add_class::<py_compiled::CompiledParser>()?;
    Ok(())
}
