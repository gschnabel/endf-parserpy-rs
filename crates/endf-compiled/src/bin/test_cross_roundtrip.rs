/// Test: compiled_read → compiled_write → interpreter_read
/// Compare compiled_read result with interpreter_read result.
use endf::parser::EndfParser;
use endf::sections::split_sections;
use endf::options::{ReadOpts, ParseOpts, WriteOpts};
use endf::value::{EndfKey, EndfValue};
use endf::records;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn compare(a: &EndfValue, b: &EndfValue, path: &str, diffs: &mut Vec<String>, max: usize) {
    if diffs.len() >= max { return; }
    match (a, b) {
        (EndfValue::Int(x), EndfValue::Int(y)) => {
            if x != y { diffs.push(format!("{}: Int {} vs {}", path, x, y)); }
        }
        (EndfValue::Float(x), EndfValue::Float(y)) => {
            if (x - y).abs() > 1e-10 + 1e-10 * y.abs() {
                diffs.push(format!("{}: Float {} vs {}", path, x, y));
            }
        }
        (EndfValue::Int(x), EndfValue::Float(y)) | (EndfValue::Float(y), EndfValue::Int(x)) => {
            if (*x as f64 - y).abs() > 1e-10 + 1e-10 * y.abs() {
                diffs.push(format!("{}: Num {} vs {}", path, x, y));
            }
        }
        (EndfValue::Str(x), EndfValue::Str(y)) => {
            if x.trim() != y.trim() { diffs.push(format!("{}: Str differs", path)); }
        }
        (EndfValue::Dict(da), EndfValue::Dict(db)) => {
            for (k, va) in da {
                let p = format!("{}/{}", path, k);
                if let Some(vb) = db.get(k) { compare(va, vb, &p, diffs, max); }
                else { diffs.push(format!("{}: key {} missing in interpreter", path, k)); }
            }
            for k in db.keys() {
                if !da.contains_key(k) {
                    diffs.push(format!("{}: key {} extra in interpreter", path, k));
                }
            }
        }
        (EndfValue::List(la), EndfValue::List(lb)) => {
            if la.len() != lb.len() {
                diffs.push(format!("{}: list len {} vs {}", path, la.len(), lb.len()));
                return;
            }
            for i in 0..la.len() {
                match (&la[i], &lb[i]) {
                    (Some(va), Some(vb)) => compare(va, vb, &format!("{}[{}]", path, i), diffs, max),
                    (None, None) => {}
                    _ => diffs.push(format!("{}[{}]: None mismatch", path, i)),
                }
            }
        }
        _ => { diffs.push(format!("{}: type mismatch", path)); }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: test_cross_roundtrip <directory>");

    let read_opts = ReadOpts {
        ignore_send_records: true,
        ignore_missing_tpid: true,
        ignore_blank_lines: true,
        ..ReadOpts::default()
    };
    let parse_opts = ParseOpts {
        ignore_number_mismatch: true,
        ignore_zero_mismatch: true,
        ignore_varspec_mismatch: true,
        ..ParseOpts::default()
    };
    let write_opts = WriteOpts::default();

    let interpreter = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_varspec_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .build()
        .unwrap();

    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("endf"))
        .map(|e| e.path())
        .collect();
    files.sort();

    let mut mf_results: BTreeMap<i32, (usize, usize, usize)> = BTreeMap::new();
    let mut total_files = 0;
    let mut file_errors = 0;

    for file in &files {
        total_files += 1;
        let fname = file.file_name().unwrap().to_str().unwrap();

        // Step 1: Read original with compiled parser
        let content = fs::read_to_string(file).unwrap().replace('\r', "");
        let lines: Vec<&str> = content.lines().collect();
        let section_map = match split_sections(&lines, &read_opts) {
            Ok(m) => m,
            Err(_) => { file_errors += 1; continue; }
        };

        for (mf, mt_map) in &section_map {
            for (mt, section_lines) in mt_map {
                let entry = mf_results.entry(*mf).or_insert((0, 0, 0));

                // Compiled read
                let compiled_data = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    endf_compiled::parse_section(*mf, *mt, section_lines, &read_opts, &parse_opts)
                })) {
                    Ok(Ok(d)) => d,
                    _ => { entry.2 += 1; continue; }
                };

                if !compiled_data.is_dict() { entry.2 += 1; continue; }

                // Compiled write
                let written_lines = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    endf_compiled::write_section(*mf, *mt, &compiled_data, &write_opts)
                })) {
                    Ok(Ok(l)) => l,
                    _ => { entry.2 += 1; continue; }
                };

                // Add SEND record and join into string for interpreter
                let mat_val = compiled_data.get("MAT")
                    .and_then(|v| v.as_int()).unwrap_or(0) as i32;
                let mut full_lines = written_lines;
                full_lines.push(records::write_send(mat_val, *mf, &write_opts));
                let endf_text = full_lines.join("\n");

                // Interpreter read
                let interp_data = match interpreter.parse(&format!("{}\n", endf_text)) {
                    Ok(d) => d,
                    Err(e) => {
                        entry.2 += 1;
                        if entry.2 <= 3 {
                            eprintln!("  {} MF{}/MT{}: interpreter parse error: {}", fname, mf, mt, e);
                        }
                        continue;
                    }
                };

                // Extract the section from interpreter result
                let interp_sec = match interp_data
                    .get(EndfKey::Int(*mf as i64))
                    .and_then(|v| v.get(EndfKey::Int(*mt as i64)))
                {
                    Some(s) if s.is_dict() => s,
                    _ => { entry.2 += 1; continue; }
                };

                // Compare compiled_read vs interpreter_read
                let mut diffs = Vec::new();
                compare(&compiled_data, interp_sec, "", &mut diffs, 5);
                if diffs.is_empty() {
                    entry.0 += 1;
                } else {
                    entry.1 += 1;
                    if entry.1 <= 3 {
                        eprintln!("  {} MF{}/MT{}: {} diffs", fname, mf, mt, diffs.len());
                        for d in diffs.iter().take(3) { eprintln!("    {}", d); }
                    }
                }
            }
        }
    }

    println!("\n=== Cross Roundtrip: compiled_read → compiled_write → interpreter_read ===");
    let (mut tok, mut td, mut te) = (0, 0, 0);
    for (mf, (ok, diffs, errs)) in &mf_results {
        let status = if *diffs == 0 && *errs == 0 { "OK" } else { "FAIL" };
        println!("  MF{:2}: {} ok={} diffs={} errors={}", mf, status, ok, diffs, errs);
        tok += ok; td += diffs; te += errs;
    }
    println!("  Total: ok={} diffs={} errors={}", tok, td, te);
    println!("  Files: {} processed, {} file-level errors", total_files, file_errors);
}
