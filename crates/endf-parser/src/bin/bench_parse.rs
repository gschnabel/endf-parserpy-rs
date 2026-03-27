
use endf_parser::parser::EndfParser;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = &args[1];
    let repeats: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(5);

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
    let data = parser.parse_file(Path::new(file)).expect("parse failed");

    // Parse benchmark
    let mut parse_times = Vec::new();
    for _ in 0..repeats {
        let t0 = Instant::now();
        let _data = parser.parse_file(Path::new(file)).expect("parse failed");
        parse_times.push(t0.elapsed().as_secs_f64());
    }
    parse_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med_parse = parse_times[repeats / 2];

    // Write benchmark
    let mut write_times = Vec::new();
    for _ in 0..repeats {
        let t0 = Instant::now();
        let _output = parser.write(&data).expect("write failed");
        write_times.push(t0.elapsed().as_secs_f64());
    }
    write_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med_write = write_times[repeats / 2];

    // Count sections
    let mf_dict = data.as_dict().unwrap();
    let n_sections: usize = mf_dict.values()
        .filter_map(|v| v.as_dict())
        .map(|d| d.len())
        .sum();

    println!("parse_median={:.6}", med_parse);
    println!("parse_min={:.6}", parse_times[0]);
    println!("parse_max={:.6}", parse_times[repeats - 1]);
    println!("write_median={:.6}", med_write);
    println!("write_min={:.6}", write_times[0]);
    println!("write_max={:.6}", write_times[repeats - 1]);
    println!("sections={}", n_sections);
}
