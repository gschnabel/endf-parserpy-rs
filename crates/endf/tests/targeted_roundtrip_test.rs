//! Targeted roundtrip test for previously-failing files only.
//! Run with: cargo test --test targeted_roundtrip_test --release -- --nocapture

use endf::parser::EndfParser;
use endf::value::{EndfValue, EndfKey};
use std::path::Path;

const ENDF_DIR: &str = "/path/to/endf/library";

const PROBLEM_FILES: &[&str] = &[
    "n-008_O_018.endf",
    "n-058_Ce_140.endf",
    "n-058_Ce_142.endf",
    "n-092_U_241.endf",
    "n-093_Np_236m1.endf",
    "n-094_Pu_241.endf",
];

fn floats_close(a: f64, b: f64) -> bool {
    if a == b { return true; }
    let diff = (a - b).abs();
    diff <= 1e-10 + 1e-10 * b.abs()
}

fn compare_values(a: &EndfValue, b: &EndfValue, path: &str, diffs: &mut Vec<String>) {
    match (a, b) {
        (EndfValue::Int(x), EndfValue::Int(y)) => {
            if x != y { diffs.push(format!("{}: int {} != {}", path, x, y)); }
        }
        (EndfValue::Float(x), EndfValue::Float(y)) => {
            if !floats_close(*x, *y) { diffs.push(format!("{}: float {} != {}", path, x, y)); }
        }
        (EndfValue::Int(x), EndfValue::Float(y)) | (EndfValue::Float(y), EndfValue::Int(x)) => {
            if !floats_close(*x as f64, *y) { diffs.push(format!("{}: num {} != {}", path, x, y)); }
        }
        (EndfValue::Str(x), EndfValue::Str(y)) => {
            if x.trim() != y.trim() { diffs.push(format!("{}: str differs", path)); }
        }
        (EndfValue::Dict(da), EndfValue::Dict(db)) => {
            for (k, va) in da {
                let child = format!("{}/{}", path, k);
                if let Some(vb) = db.get(k) {
                    compare_values(va, vb, &child, diffs);
                } else {
                    diffs.push(format!("{}: key {} missing in roundtrip", path, k));
                }
            }
            for k in db.keys() {
                if !da.contains_key(k) {
                    diffs.push(format!("{}: key {} extra in roundtrip", path, k));
                }
            }
        }
        (EndfValue::List(la), EndfValue::List(lb)) => {
            for i in 0..la.len().max(lb.len()) {
                let child = format!("{}[{}]", path, i);
                match (la.get(i), lb.get(i)) {
                    (Some(Some(va)), Some(Some(vb))) => compare_values(va, vb, &child, diffs),
                    (Some(None), Some(None)) | (None, None) => {}
                    _ => diffs.push(format!("{}: element mismatch", child)),
                }
            }
        }
        (EndfValue::Table(ta), EndfValue::Table(tb)) => {
            if ta.nbt != tb.nbt { diffs.push(format!("{}/NBT differs", path)); }
            if ta.int != tb.int { diffs.push(format!("{}/INT differs", path)); }
            let xclose = ta.x.len() == tb.x.len() && ta.x.iter().zip(&tb.x).all(|(a,b)| floats_close(*a,*b));
            let yclose = ta.y.len() == tb.y.len() && ta.y.iter().zip(&tb.y).all(|(a,b)| floats_close(*a,*b));
            if !xclose { diffs.push(format!("{}/X differs", path)); }
            if !yclose { diffs.push(format!("{}/Y differs", path)); }
        }
        _ => {
            let tl = |v: &EndfValue| match v {
                EndfValue::Int(_) => "Int", EndfValue::Float(_) => "Float",
                EndfValue::Str(_) => "Str", EndfValue::Dict(_) => "Dict",
                EndfValue::List(_) => "List", EndfValue::Table(_) => "Table",
            };
            diffs.push(format!("{}: type {} vs {}", path, tl(a), tl(b)));
        }
    }
}

#[test]
fn targeted_roundtrip() {
    let dir = Path::new(ENDF_DIR);
    if !dir.exists() {
        eprintln!("Directory not found, skipping");
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
        .unwrap();

    let mut all_ok = true;

    for fname in PROBLEM_FILES {
        let path = dir.join(fname);
        if !path.exists() { continue; }

        // Parse
        let data1 = match parser.parse_file(&path) {
            Ok(d) => d,
            Err(e) => { println!("FAIL {}: parse error: {}", fname, e); all_ok = false; continue; }
        };

        // Write
        let written = match parser.write(&data1) {
            Ok(w) => w,
            Err(e) => { println!("FAIL {}: write error: {}", fname, e); all_ok = false; continue; }
        };

        // Re-parse
        let data2 = match parser.parse(&written) {
            Ok(d) => d,
            Err(e) => { println!("FAIL {}: re-parse error: {}", fname, e); all_ok = false; continue; }
        };

        // Compare
        let mut total_diffs = 0;
        let mut total_raw = 0;
        let mut total_sections = 0;
        let mf1 = data1.as_dict().unwrap();
        let mf2 = data2.as_dict().unwrap();

        for (mf_key, mt_val1) in mf1 {
            let mt1 = match mt_val1.as_dict() { Some(d) => d, None => continue };
            let mt2 = match mf2.get(mf_key).and_then(|v| v.as_dict()) {
                Some(d) => d,
                None => { total_diffs += mt1.len(); total_sections += mt1.len(); continue; }
            };
            for (mt_key, sec1) in mt1 {
                total_sections += 1;
                if matches!(sec1, EndfValue::Str(_)) { total_raw += 1; continue; }
                let sec2 = match mt2.get(mt_key) {
                    Some(s) if !matches!(s, EndfValue::Str(_)) => s,
                    _ => { total_diffs += 1; println!("  {}: MF{}/MT{}: missing or raw in roundtrip", fname, mf_key, mt_key); continue; }
                };
                let mut diffs = Vec::new();
                compare_values(sec1, sec2, "", &mut diffs);
                if !diffs.is_empty() {
                    total_diffs += 1;
                    println!("  {}: MF{}/MT{}: {} diffs", fname, mf_key, mt_key, diffs.len());
                    for d in diffs.iter().take(3) { println!("    {}", d); }
                }
            }
        }

        if total_diffs == 0 {
            println!("OK   {}: {}/{} sections perfect ({} raw)", fname, total_sections - total_raw, total_sections, total_raw);
        } else {
            println!("FAIL {}: {} diffs, {} raw of {} sections", fname, total_diffs, total_raw, total_sections);
            all_ok = false;
        }
    }

    if all_ok {
        println!("\nAll {} problem files now pass roundtrip!", PROBLEM_FILES.len());
    } else {
        println!("\nSome files still have issues.");
    }
}
