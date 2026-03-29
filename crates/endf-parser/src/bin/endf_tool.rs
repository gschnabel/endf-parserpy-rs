//! endf-tool: Command-line interface for the ENDF-6 parser toolkit.
//!
//! Usage:
//!   endf-tool endf2json input.endf output.json [options]
//!   endf-tool json2endf input.json output.endf [options]
//!
//! Options:
//!   --format <fmt>       Recipe format: endf6, endf6-ext, jendl, pendf, errorr (default: endf6)
//!   --recipes-dir <dir>  Load recipes from a directory (overrides --format)
//!   --pretty             Pretty-print JSON output (default: compact)
//!   --lossless           Use tagged JSON format for lossless type roundtrip
//!   --ignore-mismatches  Enable all ignore_*_mismatch options
//!   --ignore-send        Ignore SEND/FEND/MEND/TEND validation
//!   --ignore-blank       Ignore blank lines
//!   --ignore-tpid        Ignore missing TPID record

use endf_parser::parser::EndfParser;
use endf_parser::json;
use endf_parser::value::EndfValue;
use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = &args[1];
    match command.as_str() {
        "endf2json" => cmd_endf2json(&args[2..]),
        "json2endf" => cmd_json2endf(&args[2..]),
        "help" | "--help" | "-h" => { print_usage(); }
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage();
            process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("endf-tool: ENDF-6 file conversion utility");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  endf-tool endf2json <input.endf> <output.json> [options]");
    eprintln!("  endf-tool json2endf <input.json> <output.endf> [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --format <fmt>       Recipe format (default: endf6)");
    eprintln!("                       Choices: endf6, endf6-ext, jendl, pendf, errorr");
    eprintln!("  --recipes-dir <dir>  Load recipes from directory (overrides --format)");
    eprintln!("  --pretty             Pretty-print JSON (default: compact)");
    eprintln!("  --lossless           Use tagged JSON for lossless type roundtrip");
    eprintln!("  --threads <n>        Parallel parsing threads (0=auto, 1=sequential, default: 1)");
    eprintln!("  --ignore-mismatches  Tolerate number/zero/varspec mismatches");
    eprintln!("  --ignore-send        Ignore SEND record validation");
    eprintln!("  --ignore-blank       Ignore blank lines");
    eprintln!("  --ignore-tpid        Ignore missing TPID record");
}

struct Options {
    input: String,
    output: String,
    format: String,
    recipes_dir: Option<String>,
    pretty: bool,
    lossless: bool,
    threads: usize,
    ignore_mismatches: bool,
    ignore_send: bool,
    ignore_blank: bool,
    ignore_tpid: bool,
}

fn parse_options(args: &[String]) -> Options {
    let mut opts = Options {
        input: String::new(),
        output: String::new(),
        format: "endf6".to_string(),
        recipes_dir: None,
        pretty: false,
        lossless: false,
        threads: 1,
        ignore_mismatches: false,
        ignore_send: false,
        ignore_blank: false,
        ignore_tpid: false,
    };

    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--format" => { i += 1; if i < args.len() { opts.format = args[i].clone(); } }
            "--recipes-dir" => { i += 1; if i < args.len() { opts.recipes_dir = Some(args[i].clone()); } }
            "--pretty" => { opts.pretty = true; }
            "--lossless" => { opts.lossless = true; }
            "--threads" => { i += 1; if i < args.len() { opts.threads = args[i].parse().unwrap_or(1); } }
            "--ignore-mismatches" => { opts.ignore_mismatches = true; }
            "--ignore-send" => { opts.ignore_send = true; }
            "--ignore-blank" => { opts.ignore_blank = true; }
            "--ignore-tpid" => { opts.ignore_tpid = true; }
            _ => { positional.push(args[i].clone()); }
        }
        i += 1;
    }

    if positional.len() >= 1 { opts.input = positional[0].clone(); }
    if positional.len() >= 2 { opts.output = positional[1].clone(); }
    opts
}

fn build_parser(opts: &Options) -> EndfParser {
    let mut builder = EndfParser::builder();

    if let Some(ref dir) = opts.recipes_dir {
        builder = builder.recipes_dir(dir.as_str());
    } else {
        builder = builder.endf_format(&opts.format);
    }

    if opts.ignore_mismatches {
        builder = builder
            .ignore_number_mismatch(true)
            .ignore_zero_mismatch(true)
            .ignore_varspec_mismatch(true);
    }
    if opts.ignore_send {
        builder = builder.ignore_send_records(true);
    }
    if opts.ignore_blank {
        builder = builder.ignore_blank_lines(true);
    }
    if opts.ignore_tpid {
        builder = builder.ignore_missing_tpid(true);
    }

    builder.build().unwrap_or_else(|e| {
        eprintln!("Error creating parser: {}", e);
        process::exit(1);
    })
}

fn cmd_endf2json(args: &[String]) {
    let opts = parse_options(args);
    if opts.input.is_empty() || opts.output.is_empty() {
        eprintln!("Usage: endf-tool endf2json <input.endf> <output.json> [options]");
        process::exit(1);
    }

    let parser = build_parser(&opts);

    // Configure rayon global pool if parallel parsing requested.
    if opts.threads != 1 {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(opts.threads)
            .build_global();
    }

    eprintln!("Parsing {}{}...", opts.input,
        if opts.threads != 1 { format!(" ({} threads)", opts.threads) } else { String::new() });
    let data = if opts.threads == 1 {
        parser.parse_file(Path::new(&opts.input))
    } else {
        parser.parse_file_parallel(Path::new(&opts.input))
    }.unwrap_or_else(|e| {
        eprintln!("Parse error: {}", e);
        process::exit(1);
    });

    eprintln!("Writing {}...", opts.output);
    let json_str = if opts.lossless {
        // Tagged format: preserves exact types (Int vs Float distinction)
        if opts.pretty {
            serde_json::to_string_pretty(&data)
        } else {
            serde_json::to_string(&data)
        }
    } else {
        // Human-friendly format (default)
        json::to_json_string(&data, opts.pretty)
    }.unwrap_or_else(|e| {
        eprintln!("JSON serialization error: {}", e);
        process::exit(1);
    });

    if opts.output == "-" {
        println!("{}", json_str);
    } else {
        std::fs::write(&opts.output, json_str).unwrap_or_else(|e| {
            eprintln!("Write error: {}", e);
            process::exit(1);
        });
    }
    eprintln!("Done.");
}

fn cmd_json2endf(args: &[String]) {
    let opts = parse_options(args);
    if opts.input.is_empty() || opts.output.is_empty() {
        eprintln!("Usage: endf-tool json2endf <input.json> <output.endf> [options]");
        process::exit(1);
    }

    let parser = build_parser(&opts);

    eprintln!("Reading {}...", opts.input);
    let json_str = std::fs::read_to_string(&opts.input).unwrap_or_else(|e| {
        eprintln!("Read error: {}", e);
        process::exit(1);
    });

    // Try human-friendly format first, fall back to lossless tagged format.
    let data: EndfValue = json::from_json_string(&json_str).or_else(|_| {
        serde_json::from_str::<EndfValue>(&json_str)
    }).unwrap_or_else(|e| {
        eprintln!("JSON parse error: {}", e);
        process::exit(1);
    });

    eprintln!("Writing {}...", opts.output);
    let endf_text = parser.write(&data).unwrap_or_else(|e| {
        eprintln!("Write error: {}", e);
        process::exit(1);
    });

    if opts.output == "-" {
        println!("{}", endf_text);
    } else {
        std::fs::write(&opts.output, endf_text).unwrap_or_else(|e| {
            eprintln!("Write error: {}", e);
            process::exit(1);
        });
    }
    eprintln!("Done.");
}
