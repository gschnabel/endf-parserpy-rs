use endf::parser::EndfParser;
use endf::options::{WriteOpts};
use endf::value::{EndfKey, EndfValue};
use std::path::Path;
use std::collections::BTreeMap;
use std::fs;

fn compare(a: &EndfValue, b: &EndfValue, path: &str, diffs: &mut Vec<String>, max: usize) {
    if diffs.len() >= max { return; }
    match (a, b) {
        (EndfValue::Int(x), EndfValue::Int(y)) => { if x != y { diffs.push(format!("{}: Int {} vs {}", path, x, y)); } }
        (EndfValue::Float(x), EndfValue::Float(y)) => { if (x-y).abs() > 1e-10+1e-10*y.abs() { diffs.push(format!("{}: Float", path)); } }
        (EndfValue::Int(x), EndfValue::Float(y)) | (EndfValue::Float(y), EndfValue::Int(x)) => {
            if (*x as f64 - y).abs() > 1e-10+1e-10*y.abs() { diffs.push(format!("{}: Num", path)); }
        }
        (EndfValue::Str(x), EndfValue::Str(y)) => { if x.trim() != y.trim() { diffs.push(format!("{}: Str", path)); } }
        (EndfValue::Dict(da), EndfValue::Dict(db)) => {
            for (k, va) in da { let p = format!("{}/{}", path, k); if let Some(vb) = db.get(k) { compare(va, vb, &p, diffs, max); } else { diffs.push(format!("{}: missing", p)); } }
            for k in db.keys() { if !da.contains_key(k) { diffs.push(format!("{}/{}: extra", path, k)); } }
        }
        (EndfValue::List(la), EndfValue::List(lb)) => {
            if la.len() != lb.len() { diffs.push(format!("{}: len {} vs {}", path, la.len(), lb.len())); return; }
            for i in 0..la.len() { match (&la[i], &lb[i]) { (Some(va), Some(vb)) => compare(va, vb, &format!("{}[{}]", path, i), diffs, max), (None, None) => {}, _ => diffs.push(format!("{}[{}]: None", path, i)) } }
        }
        _ => { diffs.push(format!("{}: type", path)); }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: test_write <directory>");
    let write_opts = WriteOpts::default();
    let ro = endf::options::ReadOpts { ignore_send_records: true, ignore_missing_tpid: true, ignore_blank_lines: true, ..Default::default() };
    let po = endf::options::ParseOpts { ignore_number_mismatch: true, ignore_zero_mismatch: true, ignore_varspec_mismatch: true, ..Default::default() };
    let parser = EndfParser::builder().ignore_number_mismatch(true).ignore_zero_mismatch(true).ignore_varspec_mismatch(true).ignore_send_records(true).ignore_missing_tpid(true).ignore_blank_lines(true).build().unwrap();

    let mut files: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("endf")).map(|e| e.path()).collect();
    files.sort();

    let mut mf_results: BTreeMap<i32, (usize, usize, usize)> = BTreeMap::new();
    for file in &files {
        let data = match parser.parse_file(file) { Ok(d) => d, Err(_) => continue };
        for (mf_key, mt_dict) in data.as_dict().unwrap() {
            let mf = match mf_key { EndfKey::Int(n) => *n as i32, _ => continue };
            if mf == 0 { continue; }
            for (mt_key, sec) in mt_dict.as_dict().unwrap() {
                let mt = match mt_key { EndfKey::Int(n) => *n as i32, _ => continue };
                if !sec.is_dict() { continue; }
                let entry = mf_results.entry(mf).or_insert((0,0,0));
                match endf_parser_compiled::write_section(mf, mt, sec, &write_opts) {
                    Ok(lines) => {
                        let lo: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
                        match endf_parser_compiled::parse_section(mf, mt, &lo, &ro, &po) {
                            Ok(rep) => { let mut d = Vec::new(); compare(sec, &rep, "", &mut d, 3); if d.is_empty() { entry.0 += 1; } else { entry.1 += 1; } }
                            Err(_) => { entry.2 += 1; }
                        }
                    }
                    Err(_) => { entry.2 += 1; }
                }
            }
        }
    }
    println!("=== Write Roundtrip (full library) ===");
    let (mut tok, mut td, mut te) = (0,0,0);
    for (mf, (ok, d, e)) in &mf_results {
        let st = if *d == 0 && *e == 0 { "OK" } else { "FAIL" };
        println!("  MF{:2}: {} ok={} diffs={} errors={}", mf, st, ok, d, e);
        tok += ok; td += d; te += e;
    }
    println!("  Total: ok={} diffs={} errors={}", tok, td, te);
}
