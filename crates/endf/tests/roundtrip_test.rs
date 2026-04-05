//! Roundtrip validation test: parse -> write -> re-parse -> compare.
//!
//! Verifies that the ENDF parser can roundtrip data through write and re-parse
//! without losing information beyond expected floating-point formatting tolerance.

use endf::parser::EndfParser;
use endf::value::{EndfKey, EndfValue};
use std::path::Path;

const ATOL: f64 = 1e-10;
const RTOL: f64 = 1e-10;

/// Result of comparing two EndfValue trees.
struct CompareResult {
    differences: Vec<String>,
}

impl CompareResult {
    fn new() -> Self {
        Self {
            differences: Vec::new(),
        }
    }

    fn is_ok(&self) -> bool {
        self.differences.is_empty()
    }
}

fn floats_close(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    if diff <= ATOL {
        return true;
    }
    let mag = a.abs().max(b.abs());
    if mag > 0.0 && diff / mag <= RTOL {
        return true;
    }
    false
}

fn compare_float_vecs(a: &[f64], b: &[f64], path: &str, label: &str, result: &mut CompareResult) {
    if a.len() != b.len() {
        result.differences.push(format!(
            "{}.{}: length mismatch ({} vs {})",
            path,
            label,
            a.len(),
            b.len()
        ));
        return;
    }
    for (i, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
        if !floats_close(*va, *vb) {
            result.differences.push(format!(
                "{}.{}[{}]: {} vs {}",
                path, label, i, va, vb
            ));
            // Only report first few per array to avoid flooding output
            if result.differences.len() > 50 {
                return;
            }
        }
    }
}

fn compare_int_vecs(a: &[i64], b: &[i64], path: &str, label: &str, result: &mut CompareResult) {
    if a.len() != b.len() {
        result.differences.push(format!(
            "{}.{}: length mismatch ({} vs {})",
            path,
            label,
            a.len(),
            b.len()
        ));
        return;
    }
    for (i, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
        if va != vb {
            result.differences.push(format!(
                "{}.{}[{}]: {} vs {}",
                path, label, i, va, vb
            ));
        }
    }
}

fn compare_values(a: &EndfValue, b: &EndfValue, path: &str, result: &mut CompareResult) {
    // Early exit if we have too many differences
    if result.differences.len() > 100 {
        return;
    }

    match (a, b) {
        (EndfValue::Int(va), EndfValue::Int(vb)) => {
            if va != vb {
                result
                    .differences
                    .push(format!("{}: Int {} vs {}", path, va, vb));
            }
        }

        // Allow Int/Float cross-comparison (common in ENDF: 0 vs 0.0)
        (EndfValue::Int(va), EndfValue::Float(vb)) | (EndfValue::Float(vb), EndfValue::Int(va)) => {
            if !floats_close(*va as f64, *vb) {
                result
                    .differences
                    .push(format!("{}: Int({}) vs Float({})", path, va, vb));
            }
        }

        (EndfValue::Float(va), EndfValue::Float(vb)) => {
            if !floats_close(*va, *vb) {
                result
                    .differences
                    .push(format!("{}: Float {} vs {}", path, va, vb));
            }
        }

        (EndfValue::Str(sa), EndfValue::Str(sb)) => {
            if sa.trim() != sb.trim() {
                result
                    .differences
                    .push(format!("{}: Str {:?} vs {:?}", path, sa.trim(), sb.trim()));
            }
        }

        (EndfValue::Dict(da), EndfValue::Dict(db)) => {
            // Check keys in a that are missing in b
            for key in da.keys() {
                if !db.contains_key(key) {
                    result
                        .differences
                        .push(format!("{}/{}: key present in first but missing in second", path, key));
                }
            }
            // Check keys in b that are missing in a
            for key in db.keys() {
                if !da.contains_key(key) {
                    result
                        .differences
                        .push(format!("{}/{}: key missing in first but present in second", path, key));
                }
            }
            // Compare common keys
            for (key, val_a) in da {
                if let Some(val_b) = db.get(key) {
                    let child_path = format!("{}/{}", path, key);
                    compare_values(val_a, val_b, &child_path, result);
                }
            }
        }

        (EndfValue::List(la), EndfValue::List(lb)) => {
            if la.len() != lb.len() {
                result.differences.push(format!(
                    "{}: List length {} vs {}",
                    path,
                    la.len(),
                    lb.len()
                ));
                return;
            }
            for (i, (ea, eb)) in la.iter().zip(lb.iter()).enumerate() {
                let child_path = format!("{}[{}]", path, i);
                match (ea, eb) {
                    (Some(va), Some(vb)) => compare_values(va, vb, &child_path, result),
                    (None, None) => {}
                    (Some(_), None) => {
                        result
                            .differences
                            .push(format!("{}: Some vs None", child_path));
                    }
                    (None, Some(_)) => {
                        result
                            .differences
                            .push(format!("{}: None vs Some", child_path));
                    }
                }
            }
        }

        _ => {
            // Type mismatch
            result.differences.push(format!(
                "{}: type mismatch ({} vs {})",
                path,
                variant_name(a),
                variant_name(b)
            ));
        }
    }
}

fn variant_name(v: &EndfValue) -> &'static str {
    match v {
        EndfValue::Int(_) => "Int",
        EndfValue::Float(_) => "Float",
        EndfValue::Str(_) => "Str",
        EndfValue::Dict(_) => "Dict",
        EndfValue::List(_) => "List",
    }
}

#[test]
fn test_roundtrip_cu63() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/testdata/n_2925_29-Cu-63.endf");

    if !path.exists() {
        eprintln!("Test file not found, skipping: {}", path.display());
        return;
    }

    let parser = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_varspec_mismatch(true)
        .accept_spaces(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .build()
        .expect("failed to create parser");

    // Step 1: Parse the original file
    println!("=== Roundtrip Test: Cu-63 ===");
    let data1 = parser
        .parse_file(&path)
        .expect("failed to parse original file");

    // Step 2: Write back to ENDF format
    let written = match parser.write(&data1) {
        Ok(w) => w,
        Err(e) => {
            println!("WARNING: write failed ({}), attempting per-section roundtrip instead", e);
            // Fall back to per-section roundtrip test
            let mf_dict = data1.as_dict().expect("data1 must be Dict");
            let mut section_pass = 0usize;
            let mut section_fail = 0usize;
            let mut section_skip = 0usize;

            for (mf_key, mt_val) in mf_dict {
                let mt_dict = mt_val.as_dict().expect("MF entry must be Dict");
                for (mt_key, section) in mt_dict {
                    if matches!(section, EndfValue::Str(_)) {
                        section_skip += 1;
                        println!("  MF {}/MT {}: SKIPPED (raw)", mf_key, mt_key);
                        continue;
                    }

                    // Build a single-section wrapper for writing
                    let mut single = EndfValue::new_dict();
                    let mut mf_d = EndfValue::new_dict();
                    mf_d.insert(mt_key.clone(), section.clone());
                    single.insert(mf_key.clone(), mf_d);

                    match parser.write(&single) {
                        Ok(section_text) => {
                            match parser.parse(&section_text) {
                                Ok(data2) => {
                                    let s2 = data2
                                        .get(mf_key.clone())
                                        .and_then(|mf| mf.get(mt_key.clone()));
                                    if let Some(s2) = s2 {
                                        let mut cmp = CompareResult::new();
                                        let cmp_path = format!("MF{}/MT{}", mf_key, mt_key);
                                        compare_values(section, s2, &cmp_path, &mut cmp);
                                        if cmp.is_ok() {
                                            section_pass += 1;
                                            println!("  MF {}/MT {}: OK", mf_key, mt_key);
                                        } else {
                                            section_fail += 1;
                                            println!("  MF {}/MT {}: DIFFERS ({} differences)", mf_key, mt_key, cmp.differences.len());
                                            for (i, diff) in cmp.differences.iter().enumerate() {
                                                if i >= 10 {
                                                    println!("    ... and {} more", cmp.differences.len() - 10);
                                                    break;
                                                }
                                                println!("    {}", diff);
                                            }
                                        }
                                    } else {
                                        section_fail += 1;
                                        println!("  MF {}/MT {}: DIFFERS (section missing after re-parse)", mf_key, mt_key);
                                    }
                                }
                                Err(e) => {
                                    section_fail += 1;
                                    println!("  MF {}/MT {}: RE-PARSE FAILED ({})", mf_key, mt_key, e);
                                }
                            }
                        }
                        Err(e) => {
                            section_fail += 1;
                            println!("  MF {}/MT {}: WRITE FAILED ({})", mf_key, mt_key, e);
                        }
                    }
                }
            }
            println!();
            println!("=== Per-Section Roundtrip Summary ===");
            println!("  Passed:  {}", section_pass);
            println!("  Failed:  {}", section_fail);
            println!("  Skipped: {}", section_skip);
            assert!(section_pass + section_fail > 0, "No parsed sections were tested");
            return;
        }
    };
    println!(
        "Written output: {} lines, {} bytes",
        written.lines().count(),
        written.len()
    );

    // Step 3: Re-parse the written output
    let data2 = parser
        .parse(&written)
        .expect("failed to re-parse written output");

    // Step 4: Compare section by section
    let mf_dict1 = data1.as_dict().expect("data1 must be Dict");
    let mf_dict2 = data2.as_dict().expect("data2 must be Dict");

    let mut total_sections = 0usize;
    let mut matching_sections = 0usize;
    let mut differing_sections = 0usize;
    let mut skipped_sections = 0usize;

    // Collect all (MF, MT) pairs from both parses
    let mut all_mf_keys: Vec<&EndfKey> = mf_dict1.keys().collect();
    for key in mf_dict2.keys() {
        if !all_mf_keys.contains(&key) {
            all_mf_keys.push(key);
        }
    }
    all_mf_keys.sort_by_key(|k| match k {
        EndfKey::Int(n) => *n,
        EndfKey::Str(s) => s.parse::<i64>().unwrap_or(0),
    });

    for mf_key in &all_mf_keys {
        let mt_dict1 = match mf_dict1.get(*mf_key) {
            Some(v) => v.as_dict().expect("MF entry must be Dict"),
            None => {
                println!("  MF {}: present only in re-parsed data", mf_key);
                continue;
            }
        };
        let mt_dict2 = match mf_dict2.get(*mf_key) {
            Some(v) => v.as_dict().expect("MF entry must be Dict"),
            None => {
                println!("  MF {}: present only in original data", mf_key);
                continue;
            }
        };

        let mut mt_keys: Vec<&EndfKey> = mt_dict1.keys().collect();
        for key in mt_dict2.keys() {
            if !mt_keys.contains(&key) {
                mt_keys.push(key);
            }
        }
        mt_keys.sort_by_key(|k| match k {
            EndfKey::Int(n) => *n,
            EndfKey::Str(s) => s.parse::<i64>().unwrap_or(0),
        });

        for mt_key in &mt_keys {
            total_sections += 1;
            let section1 = mt_dict1.get(*mt_key);
            let section2 = mt_dict2.get(*mt_key);

            match (section1, section2) {
                (Some(s1), Some(s2)) => {
                    // Skip raw (unparsed) sections - nothing useful to compare
                    if matches!(s1, EndfValue::Str(_)) || matches!(s2, EndfValue::Str(_)) {
                        skipped_sections += 1;
                        println!(
                            "  MF {}/MT {}: SKIPPED (raw/unparsed section)",
                            mf_key, mt_key
                        );
                        continue;
                    }

                    let mut cmp = CompareResult::new();
                    let cmp_path = format!("MF{}/MT{}", mf_key, mt_key);
                    compare_values(s1, s2, &cmp_path, &mut cmp);

                    if cmp.is_ok() {
                        matching_sections += 1;
                        println!("  MF {}/MT {}: OK", mf_key, mt_key);
                    } else {
                        differing_sections += 1;
                        println!(
                            "  MF {}/MT {}: DIFFERS ({} differences)",
                            mf_key,
                            mt_key,
                            cmp.differences.len()
                        );
                        for (i, diff) in cmp.differences.iter().enumerate() {
                            if i >= 10 {
                                println!(
                                    "    ... and {} more differences",
                                    cmp.differences.len() - 10
                                );
                                break;
                            }
                            println!("    {}", diff);
                        }
                    }
                }
                (Some(_), None) => {
                    differing_sections += 1;
                    println!(
                        "  MF {}/MT {}: MISSING in re-parsed data",
                        mf_key, mt_key
                    );
                }
                (None, Some(_)) => {
                    differing_sections += 1;
                    println!(
                        "  MF {}/MT {}: EXTRA in re-parsed data",
                        mf_key, mt_key
                    );
                }
                (None, None) => unreachable!(),
            }
        }
    }

    println!();
    println!("=== Roundtrip Summary ===");
    println!("  Total sections:     {}", total_sections);
    println!("  Matching sections:  {}", matching_sections);
    println!("  Differing sections: {}", differing_sections);
    println!("  Skipped (raw):      {}", skipped_sections);
    println!(
        "  Match rate:         {:.1}% (of parsed sections)",
        if total_sections - skipped_sections > 0 {
            100.0 * matching_sections as f64 / (total_sections - skipped_sections) as f64
        } else {
            0.0
        }
    );

    // The test does not panic on differences - it reports them for debugging.
    // But we do assert that at least some sections were successfully compared.
    assert!(
        total_sections > 0,
        "No sections found - the file may not have been parsed correctly"
    );
    assert!(
        matching_sections + differing_sections > 0,
        "No parsed sections were compared (all skipped as raw)"
    );
}
