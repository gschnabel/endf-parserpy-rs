use endf_parser::parser::EndfParser;
use std::fs;
use std::path::Path;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: bench_parallel <directory> [threads]");
    let num_threads: usize = std::env::args().nth(2)
        .map(|s| s.parse().unwrap())
        .unwrap_or(2);

    // Configure rayon global pool once.
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build_global()
        .unwrap();

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

    let total_bytes: u64 = files.iter().map(|f| fs::metadata(f).unwrap().len()).sum();
    let mb = total_bytes as f64 / 1024.0 / 1024.0;

    // Warmup
    let _ = parser.parse_file(&files[0]);
    let _ = parser.parse_file_parallel(&files[0]);

    // Sequential
    let t0 = Instant::now();
    let mut seq_sections = 0usize;
    let mut seq_errors = 0usize;
    for file in &files {
        match parser.parse_file(file) {
            Ok(data) => {
                let n: usize = data.as_dict().unwrap().values()
                    .filter_map(|v| v.as_dict()).map(|d| d.len()).sum();
                seq_sections += n;
            }
            Err(_) => seq_errors += 1,
        }
    }
    let seq_time = t0.elapsed().as_secs_f64();

    // Parallel
    let t0 = Instant::now();
    let mut par_sections = 0usize;
    let mut par_errors = 0usize;
    for file in &files {
        match parser.parse_file_parallel(file) {
            Ok(data) => {
                let n: usize = data.as_dict().unwrap().values()
                    .filter_map(|v| v.as_dict()).map(|d| d.len()).sum();
                par_sections += n;
            }
            Err(_) => par_errors += 1,
        }
    }
    let par_time = t0.elapsed().as_secs_f64();

    println!("Files: {}, Total: {:.1} MB, Threads: {}", files.len(), mb, num_threads);
    println!();
    println!("Sequential:  {:.1}s  {:.1} MB/s  sections={}  errors={}",
        seq_time, mb / seq_time, seq_sections, seq_errors);
    println!("Parallel:    {:.1}s  {:.1} MB/s  sections={}  errors={}",
        par_time, mb / par_time, par_sections, par_errors);
    println!("Speedup:     {:.2}x", seq_time / par_time);
}
