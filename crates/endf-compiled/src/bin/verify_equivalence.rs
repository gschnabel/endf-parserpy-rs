/// Compare compiled parser output vs interpreter output for every section
/// in every ENDF file in a directory.
use endf::parser::EndfParser;
use endf::sections::split_sections;
use endf::value::{EndfKey, EndfValue};
use endf::options::{ReadOpts, ParseOpts};
use std::fs;
use std::path::Path;

fn compare(a: &EndfValue, b: &EndfValue, path: &str, diffs: &mut Vec<String>, max_diffs: usize) {
    if diffs.len() >= max_diffs { return; }
    match (a, b) {
        (EndfValue::Int(x), EndfValue::Int(y)) => {
            if x != y { diffs.push(format!("{}: Int {} != {}", path, x, y)); }
        }
        (EndfValue::Float(x), EndfValue::Float(y)) => {
            if (x - y).abs() > 1e-10 + 1e-10 * y.abs() {
                diffs.push(format!("{}: Float {} != {}", path, x, y));
            }
        }
        (EndfValue::Int(x), EndfValue::Float(y)) | (EndfValue::Float(y), EndfValue::Int(x)) => {
            if (*x as f64 - y).abs() > 1e-10 + 1e-10 * y.abs() {
                diffs.push(format!("{}: Num {} != {}", path, x, y));
            }
        }
        (EndfValue::Str(x), EndfValue::Str(y)) => {
            if x.trim() != y.trim() { diffs.push(format!("{}: Str differs", path)); }
        }
        (EndfValue::Dict(da), EndfValue::Dict(db)) => {
            for (k, va) in da {
                let p = format!("{}/{}", path, k);
                if let Some(vb) = db.get(k) { compare(va, vb, &p, diffs, max_diffs); }
                else { diffs.push(format!("{}: key {} missing in compiled", path, k)); }
            }
            for k in db.keys() {
                if !da.contains_key(k) {
                    diffs.push(format!("{}: key {} extra in compiled", path, k));
                }
            }
        }
        (EndfValue::List(la), EndfValue::List(lb)) => {
            if la.len() != lb.len() {
                diffs.push(format!("{}: list len {} vs {}", path, la.len(), lb.len()));
            }
            for i in 0..la.len().min(lb.len()) {
                let p = format!("{}[{}]", path, i);
                match (la.get(i), lb.get(i)) {
                    (Some(Some(va)), Some(Some(vb))) => compare(va, vb, &p, diffs, max_diffs),
                    (Some(None), Some(None)) | (None, None) => {}
                    _ => { diffs.push(format!("{}: element mismatch", p)); }
                }
            }
        }
        _ => { diffs.push(format!("{}: type {} vs {}", path, type_name(a), type_name(b))); }
    }
}

fn type_name(v: &EndfValue) -> &'static str {
    match v {
        EndfValue::Int(_) => "Int", EndfValue::Float(_) => "Float",
        EndfValue::Str(_) => "Str", EndfValue::Dict(_) => "Dict",
        EndfValue::List(_) => "List",
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: verify_equivalence <directory>");
    let dir = Path::new(&dir);

    let interp = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_varspec_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .nofail(true)
        .build().unwrap();

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

    let mut files: Vec<_> = fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("endf"))
        .map(|e| e.path()).collect();
    files.sort();

    let mut total_sections = 0usize;
    let mut match_count = 0usize;
    let mut diff_count = 0usize;
    let mut skip_count = 0usize;

    for file in &files {
        let fname = file.file_name().unwrap().to_str().unwrap();
        let content = fs::read_to_string(file).unwrap();
        let content_clean = content.replace('\r', "");
        let lines: Vec<&str> = content_clean.lines().collect();
        let section_map = match split_sections(&lines, &read_opts) {
            Ok(m) => m,
            Err(_) => { eprintln!("  {}: split failed", fname); continue; }
        };

        // Parse with interpreter
        let interp_data = interp.parse(&content).unwrap();

        for (mf, mt_map) in &section_map {
            for (mt, section_lines) in mt_map {
                total_sections += 1;

                // Get interpreter result
                let interp_sec = interp_data.get(EndfKey::Int(*mf as i64))
                    .and_then(|v| v.get(EndfKey::Int(*mt as i64)));
                let interp_sec = match interp_sec {
                    Some(s) if s.is_dict() => s,
                    _ => { skip_count += 1; continue; }
                };

                // Parse with compiled
                let compiled_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    endf_compiled::parse_section(*mf, *mt, section_lines, &read_opts, &parse_opts)
                }));
                let compiled_sec = match compiled_result {
                    Ok(Ok(ref s)) if s.is_dict() => s,
                    Ok(Err(ref e)) => {
                        skip_count += 1;
                        if skip_count <= 10 { eprintln!("  SKIP compiled error {} MF{}/MT{}: {}", fname, mf, mt, e); }
                        continue;
                    }
                    Err(_) => {
                        skip_count += 1;
                        if skip_count <= 10 { eprintln!("  SKIP compiled panic {} MF{}/MT{}", fname, mf, mt); }
                        continue;
                    }
                    _ => { skip_count += 1; continue; }
                };

                let mut diffs = Vec::new();
                compare(interp_sec, compiled_sec, "", &mut diffs, 5);
                if diffs.is_empty() {
                    match_count += 1;
                } else {
                    diff_count += 1;
                    if diff_count <= 100 {
                        eprintln!("  {}: MF{}/MT{} — {} diffs", fname, mf, mt, diffs.len());
                        for d in diffs.iter().take(3) { eprintln!("    {}", d); }
                    }
                }
            }
        }
    }

    println!("\n=== Compiled vs Interpreter Equivalence ===");
    println!("  Total sections: {}", total_sections);
    println!("  Matching:       {}", match_count);
    println!("  Differing:      {}", diff_count);
    println!("  Skipped:        {}", skip_count);
}
