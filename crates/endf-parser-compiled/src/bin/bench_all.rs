//! Benchmark: Rust interpreter vs compiled parser on an ENDF library.
//!
//! Usage:
//!   bench_all <directory>

use endf::options::{ReadOpts, ParseOpts};
use endf::parser::EndfParser;
use endf::sections::split_sections;
use endf::value::{EndfKey, EndfValue};
use endf::error::EndfResult;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn compiled_parse_file(path: &Path, read_opts: &ReadOpts, parse_opts: &ParseOpts) -> EndfResult<EndfValue> {
    let content = fs::read_to_string(path)?;
    let content = content.replace('\r', "");
    let lines: Vec<&str> = content.lines().collect();
    let section_map = split_sections(&lines, read_opts)?;

    let mut result = EndfValue::new_dict();
    for (mf, mt_map) in &section_map {
        let mut mf_dict = EndfValue::new_dict();
        for (mt, section_lines) in mt_map {
            let sl = section_lines.clone();
            let ro = read_opts.clone();
            let po = parse_opts.clone();
            let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                endf_parser_compiled::parse_section(*mf, *mt, &sl, &ro, &po)
            }));
            match parsed {
                Ok(Ok(data)) => { mf_dict.insert(EndfKey::Int(*mt as i64), data); }
                _ => {
                    let raw = EndfValue::Str(section_lines.join("\n"));
                    mf_dict.insert(EndfKey::Int(*mt as i64), raw);
                }
            }
        }
        result.insert(EndfKey::Int(*mf as i64), mf_dict);
    }
    Ok(result)
}

fn collect_files(target: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(target).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("endf"))
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}

fn bench<F>(name: &str, files: &[PathBuf], total_mb: f64, mut parse_fn: F)
where
    F: FnMut(&Path) -> EndfResult<EndfValue>,
{
    // warmup
    let _ = parse_fn(&files[0]);

    let t0 = Instant::now();
    let mut errors = 0usize;
    for file in files {
        if parse_fn(file).is_err() {
            errors += 1;
        }
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let rate = total_mb / elapsed;
    println!("| {:<45} | {:6.1}s | {:6.1} MB/s | {:>6} |", name, elapsed, rate, errors);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: bench_all <directory>");
        std::process::exit(1);
    }

    let target = Path::new(&args[1]);
    let files = collect_files(target);
    let total_bytes: u64 = files.iter().map(|f| fs::metadata(f).unwrap().len()).sum();
    let total_mb = total_bytes as f64 / 1024.0 / 1024.0;

    println!("Library: {} files, {:.1} MB\n", files.len(), total_mb);

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

    println!("| {:<45} | {:>6} | {:>10} | {:>6} |", "Parser", "Time", "Speed", "Errors");
    println!("|{:-<47}|{:-<8}|{:-<12}|{:-<8}|", "", "", "", "");

    // Rust interpreter (endf6-ext)
    let parser = EndfParser::builder()
        .endf_format("endf6-ext")
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_varspec_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .build()
        .unwrap();

    bench("Rust interpreter (endf6-ext)", &files, total_mb, |path| {
        parser.parse_file(path)
    });

    // Compiled Rust parser (endf6)
    let ro = read_opts.clone();
    let po = parse_opts.clone();
    bench("Rust compiled parser (endf6)", &files, total_mb, |path| {
        compiled_parse_file(path, &ro, &po)
    });
}
