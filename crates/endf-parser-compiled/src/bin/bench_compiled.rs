use endf_parser::options::{ReadOpts, ParseOpts};
use endf_parser::sections::split_sections;
use endf_parser::value::{EndfKey, EndfValue};
use endf_parser::error::EndfResult;
use std::fs;
use std::path::Path;
use std::time::Instant;

fn parse_file(path: &Path, read_opts: &ReadOpts, parse_opts: &ParseOpts) -> EndfResult<EndfValue> {
    let content = fs::read_to_string(path)?;
    let content = content.replace('\r', "");
    let lines: Vec<&str> = content.lines().collect();
    let section_map = split_sections(&lines, read_opts)?;

    let mut result = EndfValue::new_dict();
    for (mf, mt_map) in &section_map {
        let mut mf_dict = EndfValue::new_dict();
        for (mt, section_lines) in mt_map {
            // Use catch_unwind to handle panics from generated code (e.g., index out of bounds)
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: bench_compiled <directory-or-file> [repeats]");
        std::process::exit(1);
    }
    let target = Path::new(&args[1]);
    let repeats: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(1);

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

    let files: Vec<_> = if target.is_dir() {
        let mut f: Vec<_> = fs::read_dir(target).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("endf"))
            .map(|e| e.path())
            .collect();
        f.sort();
        f
    } else {
        vec![target.to_path_buf()]
    };

    let total_bytes: u64 = files.iter().map(|f| fs::metadata(f).unwrap().len()).sum();

    // Warmup
    let _ = parse_file(&files[0], &read_opts, &parse_opts);

    let mut best_time = f64::MAX;
    for _ in 0..repeats {
        let t0 = Instant::now();
        let mut total_sections = 0usize;
        let mut errors = 0usize;
        for file in &files {
            match parse_file(file, &read_opts, &parse_opts) {
                Ok(data) => {
                    let n: usize = data.as_dict().unwrap().values()
                        .filter_map(|v| v.as_dict()).map(|d| d.len()).sum();
                    total_sections += n;
                }
                Err(_) => errors += 1,
            }
        }
        let elapsed = t0.elapsed().as_secs_f64();
        if elapsed < best_time { best_time = elapsed; }
        let mb = total_bytes as f64 / 1024.0 / 1024.0;
        eprintln!("  run: {:.3}s  ({:.1} MB/s)  sections={} errors={}", elapsed, mb / elapsed, total_sections, errors);
    }

    let mb = total_bytes as f64 / 1024.0 / 1024.0;
    println!("files={}", files.len());
    println!("time={:.3}", best_time);
    println!("mb={:.1}", mb);
    println!("rate={:.1}", mb / best_time);
}
