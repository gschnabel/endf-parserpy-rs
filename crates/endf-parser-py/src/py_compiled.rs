use pyo3::prelude::*;
use pyo3::types::PyDict;
use rayon::prelude::*;
use std::path::Path;

use endf_parser::options::{ReadOpts, ParseOpts, WriteOpts};
use endf_parser::sections::split_sections;
use endf_parser::value::{EndfKey, EndfValue};
use endf_parser::records;
use super::py_value::{endf_value_to_py, py_to_endf_value};

#[pyclass(name = "CompiledParser")]
pub struct CompiledParser {
    read_opts: ReadOpts,
    parse_opts: ParseOpts,
    write_opts: WriteOpts,
}

#[pymethods]
impl CompiledParser {
    #[new]
    #[pyo3(signature = (**kwargs))]
    fn new(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut read_opts = ReadOpts::default();
        let mut parse_opts = ParseOpts::default();
        let mut write_opts = WriteOpts::default();

        if let Some(kw) = kwargs {
            if let Some(v) = kw.get_item("ignore_number_mismatch")? {
                parse_opts.ignore_number_mismatch = v.extract()?;
            }
            if let Some(v) = kw.get_item("ignore_zero_mismatch")? {
                parse_opts.ignore_zero_mismatch = v.extract()?;
            }
            if let Some(v) = kw.get_item("ignore_varspec_mismatch")? {
                parse_opts.ignore_varspec_mismatch = v.extract()?;
            }
            if let Some(v) = kw.get_item("accept_spaces")? {
                read_opts.accept_spaces = v.extract()?;
            }
            if let Some(v) = kw.get_item("ignore_blank_lines")? {
                read_opts.ignore_blank_lines = v.extract()?;
            }
            if let Some(v) = kw.get_item("ignore_send_records")? {
                read_opts.ignore_send_records = v.extract()?;
            }
            if let Some(v) = kw.get_item("ignore_missing_tpid")? {
                read_opts.ignore_missing_tpid = v.extract()?;
            }
            if let Some(v) = kw.get_item("abuse_signpos")? {
                write_opts.abuse_signpos = v.extract()?;
            }
            if let Some(v) = kw.get_item("skip_intzero")? {
                write_opts.skip_intzero = v.extract()?;
            }
            if let Some(v) = kw.get_item("prefer_noexp")? {
                write_opts.prefer_noexp = v.extract()?;
            }
            if let Some(v) = kw.get_item("keep_E")? {
                write_opts.keep_e = v.extract()?;
            }
            if let Some(v) = kw.get_item("include_linenum")? {
                write_opts.include_linenum = v.extract()?;
            }
        }

        Ok(Self { read_opts, parse_opts, write_opts })
    }

    /// Parse an ENDF file using the compiled parser.
    /// Phase 1 (parallel, GIL released): parse all sections in Rust.
    /// Phase 2 (sequential, GIL held): convert EndfValue to Python objects.
    fn parsefile(&self, py: Python<'_>, filename: &str) -> PyResult<PyObject> {
        let content = std::fs::read_to_string(filename).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string())
        })?;
        self.parse(py, &content)
    }

    /// Parse ENDF text using the compiled parser with parallel section parsing.
    fn parse(&self, py: Python<'_>, input: &str) -> PyResult<PyObject> {
        let content = input.replace('\r', "");

        // Phase 1: parse all sections in parallel, GIL released.
        let read_opts = self.read_opts.clone();
        let parse_opts = self.parse_opts.clone();
        let result = py.allow_threads(|| -> Result<EndfValue, String> {
            let lines: Vec<&str> = content.lines().collect();
            let section_map = split_sections(&lines, &read_opts)
                .map_err(|e| e.to_string())?;

            // Flatten into (mf, mt, section_lines) for parallel processing.
            let sections: Vec<(i32, i32, Vec<String>)> = section_map.iter()
                .flat_map(|(mf, mt_map)| {
                    mt_map.iter().map(move |(mt, sl)| (*mf, *mt, sl.clone()))
                })
                .collect();

            // Parse sections in parallel.
            let parsed: Vec<(i32, i32, EndfValue)> = sections.par_iter()
                .map(|(mf, mt, sl)| {
                    let ro = read_opts.clone();
                    let po = parse_opts.clone();
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        endf_parser_compiled::parse_section(*mf, *mt, sl, &ro, &po)
                    })) {
                        Ok(Ok(data)) => (*mf, *mt, data),
                        _ => (*mf, *mt, EndfValue::Str(sl.join("\n"))),
                    }
                })
                .collect();

            // Assemble into nested dict.
            let mut result = EndfValue::new_dict();
            for (mf, mt, data) in parsed {
                if !result.contains_key(EndfKey::Int(mf as i64)) {
                    result.insert(EndfKey::Int(mf as i64), EndfValue::new_dict());
                }
                let mf_dict = result.get_mut(EndfKey::Int(mf as i64)).unwrap();
                mf_dict.insert(EndfKey::Int(mt as i64), data);
            }
            Ok(result)
        }).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)
        })?;

        // Phase 2: convert EndfValue to Python objects (requires GIL).
        endf_value_to_py(py, &result)
    }

    /// Write structured data back to ENDF format using the compiled writer.
    fn write(&self, endf_dict: &Bound<'_, PyAny>) -> PyResult<String> {
        let data = py_to_endf_value(endf_dict)?;
        let mf_dict = data.as_dict().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>("top-level value must be a dict")
        })?;

        let mut all_lines: Vec<String> = Vec::new();
        let mut mat = 0i32;

        // Determine MAT
        for (_mf_key, mt_dict_val) in mf_dict {
            if let Some(mt_d) = mt_dict_val.as_dict() {
                for (_mt_key, section_data) in mt_d {
                    if let Some(m) = section_data.get("MAT").and_then(|v| v.as_int()) {
                        if m > 0 { mat = m as i32; break; }
                    }
                }
            }
            if mat > 0 { break; }
        }

        for (mf_key, mt_dict_val) in mf_dict {
            let mf = match mf_key {
                EndfKey::Int(n) => *n as i32,
                _ => continue,
            };

            // MF0 is TPID
            if mf == 0 {
                if let Some(mt_d) = mt_dict_val.as_dict() {
                    for (_mt_key, section_data) in mt_d {
                        if let Some(EndfValue::Str(ref text)) = section_data.get("TPID") {
                            let ctrl = records::CtrlRecord { mat: 0, mf: 0, mt: 0 };
                            let rec = records::TextRecord { text: text.clone() };
                            all_lines.push(records::write_text(&rec, &ctrl, &self.write_opts));
                        }
                    }
                }
                continue;
            }

            let mt_dict = match mt_dict_val.as_dict() {
                Some(d) => d,
                None => continue,
            };

            for (mt_key, section_data) in mt_dict {
                let mt = match mt_key {
                    EndfKey::Int(n) => *n as i32,
                    _ => continue,
                };

                if let EndfValue::Str(raw) = section_data {
                    all_lines.extend(raw.lines().map(String::from));
                } else {
                    match endf_parser_compiled::write_section(mf, mt, section_data, &self.write_opts) {
                        Ok(mut section_lines) => {
                            endf_parser::sections::add_linenumbers(&mut section_lines, mf, &self.write_opts);
                            all_lines.extend(section_lines);
                        }
                        Err(e) => {
                            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                                format!("write error MF{}/MT{}: {}", mf, mt, e)
                            ));
                        }
                    }
                }
                all_lines.push(records::write_send(mat, mf, &self.write_opts));
            }
            all_lines.push(records::write_fend(mat, &self.write_opts));
        }
        all_lines.push(records::write_mend(&self.write_opts));
        all_lines.push(records::write_tend(&self.write_opts));

        Ok(all_lines.join("\n"))
    }

    /// Write structured data to a file using the compiled writer.
    fn writefile(&self, filename: &str, endf_dict: &Bound<'_, PyAny>) -> PyResult<()> {
        let content = self.write(endf_dict)?;
        std::fs::write(filename, content).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string())
        })
    }
}
