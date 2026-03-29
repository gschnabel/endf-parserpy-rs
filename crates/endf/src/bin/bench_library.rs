use endf::parser::EndfParser;
use std::fs;
use std::path::Path;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: bench_library <directory>");
    let dir = Path::new(&dir);

    let mut files: Vec<_> = fs::read_dir(dir)
        .expect("cannot read directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("endf"))
        .map(|e| e.path())
        .collect();
    files.sort();

    let parser = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_varspec_mismatch(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .nofail(true)
        .build()
        .expect("parser build failed");

    // Warmup
    let _ = parser.parse_file(&files[0]);

    let total_bytes: u64 = files.iter().map(|f| fs::metadata(f).unwrap().len()).sum();

    let t0 = Instant::now();
    let mut total_sections = 0usize;
    let mut errors = 0usize;
    for file in &files {
        match parser.parse_file(file) {
            Ok(data) => {
                let n: usize = data
                    .as_dict()
                    .unwrap()
                    .values()
                    .filter_map(|v| v.as_dict())
                    .map(|d| d.len())
                    .sum();
                total_sections += n;
            }
            Err(_) => errors += 1,
        }
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let mb = total_bytes as f64 / 1024.0 / 1024.0;

    println!("files={}", files.len());
    println!("sections={}", total_sections);
    println!("errors={}", errors);
    println!("time={:.3}", elapsed);
    println!("mb={:.1}", mb);
    println!("rate={:.1}", mb / elapsed);
}
