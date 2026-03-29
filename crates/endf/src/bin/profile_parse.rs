use endf::parser::EndfParser;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = &args[1];

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

    // Single parse - profiler will capture this
    let data = parser.parse_file(Path::new(file)).expect("parse failed");

    // Prevent dead-code elimination
    let mf_dict = data.as_dict().unwrap();
    let n: usize = mf_dict.values().filter_map(|v| v.as_dict()).map(|d| d.len()).sum();
    eprintln!("Parsed {} sections", n);
}
