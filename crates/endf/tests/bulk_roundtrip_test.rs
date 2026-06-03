//! Bulk roundtrip test: parse → write → re-parse → compare for every .endf
//! file in a directory.  Run with:
//!
//!   cargo test --test bulk_roundtrip_test --release -- --nocapture --ignored

use endf::parser::EndfParser;
use endf::value::{EndfKey, EndfValue};
use std::fs;
use std::path::Path;

// ── value comparison ────────────────────────────────────────────────────

fn compare_values(a: &EndfValue, b: &EndfValue, path: &str, diffs: &mut Vec<String>) {
    match (a, b) {
        (EndfValue::Int(x), EndfValue::Int(y)) => {
            if x != y {
                diffs.push(format!("{}: int {} != {}", path, x, y));
            }
        }
        (EndfValue::Float(x), EndfValue::Float(y))
        | (EndfValue::PreservedFloat(x, _), EndfValue::Float(y))
        | (EndfValue::Float(x), EndfValue::PreservedFloat(y, _))
        | (EndfValue::PreservedFloat(x, _), EndfValue::PreservedFloat(y, _)) => {
            if !floats_close(*x, *y) {
                diffs.push(format!("{}: float {} != {}", path, x, y));
            }
        }
        // allow Int/Float cross-comparison (0 == 0.0 etc.)
        (EndfValue::Int(x), EndfValue::Float(y)) | (EndfValue::Float(y), EndfValue::Int(x))
        | (EndfValue::Int(x), EndfValue::PreservedFloat(y, _)) | (EndfValue::PreservedFloat(y, _), EndfValue::Int(x)) => {
            if !floats_close(*x as f64, *y) {
                diffs.push(format!("{}: num {} != {}", path, x, y));
            }
        }
        (EndfValue::Str(x), EndfValue::Str(y)) => {
            if x.trim() != y.trim() {
                diffs.push(format!("{}: str '{}' != '{}'", path, x.trim(), y.trim()));
            }
        }
        (EndfValue::Dict(da), EndfValue::Dict(db)) => {
            for (k, va) in da {
                let child = format!("{}/{}", path, k);
                if let Some(vb) = db.get(k) {
                    compare_values(va, vb, &child, diffs);
                } else {
                    diffs.push(format!("{}: key {} missing in second", path, k));
                }
            }
            for k in db.keys() {
                if !da.contains_key(k) {
                    diffs.push(format!("{}: key {} missing in first", path, k));
                }
            }
        }
        (EndfValue::List(la), EndfValue::List(lb)) => {
            let len = la.len().max(lb.len());
            for i in 0..len {
                let child = format!("{}[{}]", path, i);
                match (la.get(i), lb.get(i)) {
                    (Some(Some(va)), Some(Some(vb))) => compare_values(va, vb, &child, diffs),
                    (Some(None), Some(None)) => {}
                    (None, None) => {}
                    _ => diffs.push(format!("{}: list element mismatch", child)),
                }
            }
        }
        _ => {
            diffs.push(format!("{}: type mismatch ({} vs {})", path, type_label(a), type_label(b)));
        }
    }
}

fn floats_close(a: f64, b: f64) -> bool {
    if a == b { return true; }
    let diff = (a - b).abs();
    diff <= 1e-10 + 1e-10 * b.abs()
}

fn type_label(v: &EndfValue) -> &'static str {
    match v {
        EndfValue::Int(_) => "Int",
        EndfValue::Float(_) | EndfValue::PreservedFloat(_, _) => "Float",
        EndfValue::Str(_) => "Str",
        EndfValue::Dict(_) => "Dict",
        EndfValue::List(_) => "List",
    }
}

// ── the actual test ─────────────────────────────────────────────────────

const ENDF_DIR: &str = "/path/to/endf/library";

#[test]
#[ignore] // run explicitly with --ignored
fn debug_diff_details() {
    let parser = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .nofail(true)
        .build()
        .expect("parser build failed");

    // All 17 problem files from the bulk test
    let test_files = vec![
        "n-003_Li_007.endf",
        "n-005_B_010.endf",
        "n-005_B_011.endf",
        "n-006_C_012.endf",
        "n-008_O_016.endf",
        "n-011_Na_023.endf",
        "n-013_Al_027.endf",
        "n-014_Si_028.endf",
        "n-014_Si_029.endf",
        "n-014_Si_030.endf",
        "n-024_Cr_050.endf",
        "n-024_Cr_052.endf",
        "n-024_Cr_053.endf",
        "n-024_Cr_054.endf",
        "n-026_Fe_054.endf",
        "n-026_Fe_057.endf",
        "n-029_Cu_063.endf",
    ];

    let dir = Path::new(ENDF_DIR);

    for fname in &test_files {
        let path = dir.join(fname);
        if !path.exists() {
            eprintln!("SKIP (not found): {}", fname);
            continue;
        }

        let data1 = match parser.parse_file(&path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("PARSE FAIL {}: {}", fname, e);
                continue;
            }
        };

        let written = match parser.write(&data1) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("WRITE FAIL {}: {}", fname, e);
                continue;
            }
        };

        let data2 = match parser.parse(&written) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("RE-PARSE FAIL {}: {}", fname, e);
                continue;
            }
        };

        // Compare section by section and print diffs
        let mf_dict1 = data1.as_dict().unwrap();
        let mf_dict2 = data2.as_dict().unwrap();

        let mut total_secs = 0usize;
        let mut total_diffs_file = 0usize;
        for (mf_key, mt_val1) in mf_dict1 {
            let mt_dict1 = match mt_val1.as_dict() {
                Some(d) => d,
                None => continue,
            };
            let mt_dict2 = match mf_dict2.get(mf_key).and_then(|v| v.as_dict()) {
                Some(d) => d,
                None => continue,
            };

            for (mt_key, sec1) in mt_dict1 {
                if matches!(sec1, EndfValue::Str(_)) {
                    continue;
                }
                total_secs += 1;
                let sec2 = match mt_dict2.get(mt_key) {
                    Some(s) if !matches!(s, EndfValue::Str(_)) => s,
                    _ => {
                        total_diffs_file += 1;
                        println!("  {} MF={} MT={}: second is missing or raw", fname, mf_key, mt_key);
                        continue;
                    },
                };

                let mut diffs = Vec::new();
                compare_values(sec1, sec2, "", &mut diffs);
                if !diffs.is_empty() {
                    total_diffs_file += 1;
                    println!("\n=== {} MF={} MT={} : {} diffs ===", fname, mf_key, mt_key, diffs.len());
                    for d in &diffs {
                        println!("  {}", d);
                    }
                }
            }
        }
        println!("{}: {} sections, {} with diffs", fname, total_secs, total_diffs_file);
    }
}

#[test]
#[ignore]
fn debug_mt151_reparse() {
    let parser_nofail = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .nofail(true)
        .build()
        .expect("parser build failed");

    let dir = Path::new(ENDF_DIR);
    let path = dir.join("n-014_Si_028.endf");

    let data1 = parser_nofail.parse_file(&path).expect("parse failed");

    // Build a data value containing only MF2/MT151
    let mt151_data = data1.as_dict().unwrap()
        .get(&EndfKey::Int(2)).unwrap().as_dict().unwrap()
        .get(&EndfKey::Int(151)).unwrap().clone();

    let mut mt_dict = EndfValue::new_dict();
    mt_dict.insert(EndfKey::Int(151), mt151_data);
    let mut mf_dict = EndfValue::new_dict();
    mf_dict.insert(EndfKey::Int(2), mt_dict);

    // Try writing with a strict parser (no nofail)
    let parser_strict = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .nofail(false)
        .build()
        .expect("parser build failed");

    match parser_strict.write(&mf_dict) {
        Ok(written) => {
            let all_lines: Vec<&str> = written.lines().collect();
            println!("Total lines in written output: {}", all_lines.len());
            for (i, line) in all_lines.iter().enumerate().take(10) {
                println!("  L{}: len={} '{}'", i+1, line.len(), line);
            }
            if all_lines.len() > 10 {
                println!("  ...");
                for (i, line) in all_lines.iter().enumerate().skip(all_lines.len()-5) {
                    println!("  L{}: len={} '{}'", i+1, line.len(), line);
                }
            }
        },
        Err(e) => println!("STRICT WRITE FAILED: {}", e),
    }
}

#[test]
#[ignore]
fn bulk_roundtrip_all_endfb81() {
    let dir = Path::new(ENDF_DIR);
    if !dir.exists() {
        eprintln!("Directory not found, skipping: {}", ENDF_DIR);
        return;
    }

    let parser = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .nofail(true)
        .build()
        .expect("parser build failed");

    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("endf"))
        .collect();
    entries.sort_by_key(|e| e.path());

    let total_files = entries.len();
    let mut files_perfect = 0usize;
    let mut files_with_diffs = 0usize;
    let mut files_parse_fail = 0usize;
    let mut files_write_fail = 0usize;
    let mut total_sections = 0usize;
    let mut total_sections_ok = 0usize;
    let mut total_sections_raw = 0usize;
    let mut total_sections_diff = 0usize;
    let mut problem_files: Vec<(String, String)> = Vec::new();

    for entry in &entries {
        let path = entry.path();
        let fname = path.file_name().unwrap().to_str().unwrap().to_string();

        // 1. Parse
        let data1 = match parser.parse_file(&path) {
            Ok(d) => d,
            Err(e) => {
                files_parse_fail += 1;
                problem_files.push((fname.clone(), format!("parse: {}", e)));
                continue;
            }
        };

        // 2. Write
        let written = match parser.write(&data1) {
            Ok(w) => w,
            Err(e) => {
                files_write_fail += 1;
                problem_files.push((fname.clone(), format!("write: {}", e)));
                continue;
            }
        };

        // 3. Re-parse
        let data2 = match parser.parse(&written) {
            Ok(d) => d,
            Err(e) => {
                files_write_fail += 1;
                problem_files.push((fname.clone(), format!("re-parse: {}", e)));
                continue;
            }
        };

        // 4. Compare section by section
        let mf_dict1 = data1.as_dict().unwrap();
        let mf_dict2 = data2.as_dict().unwrap();
        let mut file_diffs = 0usize;
        let mut file_raw = 0usize;
        let mut file_sections = 0usize;

        for (mf_key, mt_val1) in mf_dict1 {
            let mt_dict1 = match mt_val1.as_dict() {
                Some(d) => d,
                None => continue,
            };
            let mt_dict2 = match mf_dict2.get(mf_key).and_then(|v| v.as_dict()) {
                Some(d) => d,
                None => {
                    // entire MF missing in roundtrip
                    file_diffs += mt_dict1.len();
                    file_sections += mt_dict1.len();
                    continue;
                }
            };

            for (mt_key, sec1) in mt_dict1 {
                file_sections += 1;
                total_sections += 1;

                if matches!(sec1, EndfValue::Str(_)) {
                    file_raw += 1;
                    total_sections_raw += 1;
                    continue;
                }

                let sec2 = match mt_dict2.get(mt_key) {
                    Some(s) => s,
                    None => {
                        file_diffs += 1;
                        total_sections_diff += 1;
                        continue;
                    }
                };

                if matches!(sec2, EndfValue::Str(_)) {
                    file_diffs += 1;
                    total_sections_diff += 1;
                    continue;
                }

                let mut diffs = Vec::new();
                compare_values(sec1, sec2, "", &mut diffs);
                if diffs.is_empty() {
                    total_sections_ok += 1;
                } else {
                    file_diffs += 1;
                    total_sections_diff += 1;
                }
            }
        }

        if file_diffs == 0 && file_raw == 0 {
            files_perfect += 1;
        } else if file_diffs > 0 {
            files_with_diffs += 1;
            problem_files.push((fname.clone(), format!("{} diffs, {} raw of {} sections", file_diffs, file_raw, file_sections)));
        } else {
            // only raw sections, no diffs among parsed ones
            files_perfect += 1;
        }
    }

    // ── report ──
    println!("\n============================================================");
    println!("  ENDF/B-VIII.1 Bulk Roundtrip Results");
    println!("============================================================");
    println!();
    println!("Files:     {} total", total_files);
    println!("  Perfect: {}", files_perfect);
    println!("  Diffs:   {}", files_with_diffs);
    println!("  Parse fail: {}", files_parse_fail);
    println!("  Write fail: {}", files_write_fail);
    println!();
    println!("Sections:  {} total", total_sections);
    println!("  OK:      {}", total_sections_ok);
    println!("  Diff:    {}", total_sections_diff);
    println!("  Raw:     {}", total_sections_raw);
    if total_sections > 0 {
        let pct = 100.0 * total_sections_ok as f64 / (total_sections - total_sections_raw) as f64;
        println!("  Match rate: {:.1}% (of parsed sections)", pct);
    }

    if !problem_files.is_empty() {
        println!();
        println!("Problem files ({}):", problem_files.len());
        for (f, reason) in &problem_files {
            println!("  {}: {}", f, reason);
        }
    }
    println!();
}
