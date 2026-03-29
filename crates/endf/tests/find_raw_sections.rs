use endf::parser::EndfParser;
use endf::value::{EndfValue, EndfKey};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const ENDF_DIR: &str = "/path/to/endf/library";

#[test]
#[ignore]
fn find_raw_mfmt_sections() {
    let dir = Path::new(ENDF_DIR);
    if !dir.exists() { return; }

    let parser = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .nofail(true)
        .build()
        .unwrap();

    // (mf, mt) → list of files containing it as raw
    let mut raw_map: BTreeMap<(i64, i64), Vec<String>> = BTreeMap::new();

    let mut entries: Vec<_> = fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("endf"))
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in &entries {
        let path = entry.path();
        let fname = path.file_name().unwrap().to_str().unwrap().to_string();
        let data = match parser.parse_file(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let mf_dict = data.as_dict().unwrap();
        for (mf_key, mt_val) in mf_dict {
            let mf = match mf_key { EndfKey::Int(n) => *n, _ => continue };
            let mt_dict = match mt_val.as_dict() { Some(d) => d, None => continue };
            for (mt_key, section) in mt_dict {
                let mt = match mt_key { EndfKey::Int(n) => *n, _ => continue };
                if matches!(section, EndfValue::Str(_)) {
                    raw_map.entry((mf, mt)).or_default().push(fname.clone());
                }
            }
        }
    }

    println!("\nRaw (recipe-less) MF/MT sections across 558 ENDF/B-VIII.1 files:\n");
    println!("{:<6} {:<6} {:<8} {}", "MF", "MT", "Count", "Example files");
    println!("{}", "-".repeat(70));
    for ((mf, mt), files) in &raw_map {
        let examples: Vec<&str> = files.iter().take(3).map(|s| s.as_str()).collect();
        let suffix = if files.len() > 3 { format!(", ... ({} total)", files.len()) } else { String::new() };
        println!("{:<6} {:<6} {:<8} {}{}", mf, mt, files.len(), examples.join(", "), suffix);
    }
    println!("\nTotal: {} distinct MF/MT combinations, {} raw section instances",
        raw_map.len(),
        raw_map.values().map(|v| v.len()).sum::<usize>());
}
