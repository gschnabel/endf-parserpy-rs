//! endf-tool: Command-line interface for the ENDF-6 parser toolkit.
//!
//! Subcommands:
//!   endf2json   Convert ENDF to JSON
//!   json2endf   Convert JSON to ENDF
//!   validate    Validate ENDF files for compliance
//!   compare     Compare two ENDF files
//!   convert     Convert between ENDF and JSON (unified)

use clap::{Args, Parser, Subcommand};
use endf::json;
use endf::parser::EndfParser;
use endf::value::EndfValue;
use std::path::Path;
use std::process;

// ---------------------------------------------------------------------------
// CLI structure
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "endf-tool", about = "ENDF-6 file conversion and analysis utility")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert ENDF to JSON
    Endf2json {
        /// Input ENDF file (or - for stdin)
        input: String,
        /// Output JSON file (or - for stdout)
        output: String,
        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,
        /// Use tagged JSON for lossless type roundtrip
        #[arg(long)]
        lossless: bool,
        /// Number of parallel parsing threads (0=auto, 1=sequential)
        #[arg(long, default_value = "1")]
        threads: usize,
        #[command(flatten)]
        common: CommonOpts,
    },
    /// Convert JSON to ENDF
    Json2endf {
        /// Input JSON file
        input: String,
        /// Output ENDF file (or - for stdout)
        output: String,
        /// Number of parallel parsing threads (0=auto, 1=sequential)
        #[arg(long, default_value = "1")]
        threads: usize,
        #[command(flatten)]
        common: CommonOpts,
    },
    /// Convert between ENDF and JSON (unified)
    Convert {
        /// Source file
        source: String,
        /// Destination file
        dest: String,
        /// Destination format
        #[arg(long, value_parser = ["endf", "json"])]
        to: String,
        /// JSON indent level (only for --to json)
        #[arg(long)]
        indent: Option<usize>,
        /// Use tagged JSON for lossless type roundtrip
        #[arg(long)]
        lossless: bool,
        /// Number of parallel parsing threads (0=auto, 1=sequential)
        #[arg(long, default_value = "1")]
        threads: usize,
        #[command(flatten)]
        common: CommonOpts,
    },
    /// Validate ENDF files for compliance
    Validate {
        /// ENDF files to validate (glob patterns supported)
        files: Vec<String>,
        #[command(flatten)]
        common: CommonOpts,
    },
    /// Compare two ENDF files
    Compare {
        /// First ENDF file
        file1: String,
        /// Second ENDF file
        file2: String,
        /// Absolute tolerance for float comparison
        #[arg(long, default_value = "1e-8")]
        atol: f64,
        /// Relative tolerance for float comparison
        #[arg(long, default_value = "1e-6")]
        rtol: f64,
        #[command(flatten)]
        common: CommonOpts,
    },
}

/// Common parser/writer options shared across all subcommands.
/// These mirror the Python endf-parserpy CLI flags.
#[derive(Args, Clone)]
struct CommonOpts {
    /// ENDF format flavour (endf6, endf6-ext, jendl, pendf, errorr)
    #[arg(long, default_value = "endf6-ext")]
    endf_format: String,
    /// Load recipes from directory (overrides --endf-format)
    #[arg(long)]
    recipes_dir: Option<String>,
    /// Tolerate number mismatches in constant fields
    #[arg(long)]
    ignore_number_mismatch: bool,
    /// Tolerate non-zero values in zero-expected fields
    #[arg(long)]
    ignore_zero_mismatch: bool,
    /// Tolerate inconsistent variable assignments
    #[arg(long)]
    ignore_varspec_mismatch: bool,
    /// Use approximate comparison for field validation
    #[arg(long)]
    fuzzy_matching: bool,
    /// Strip spaces inside numeric fields
    #[arg(long)]
    accept_spaces: bool,
    /// Skip blank lines without error
    #[arg(long)]
    ignore_blank_lines: bool,
    /// Skip SEND/FEND/MEND/TEND validation
    #[arg(long)]
    ignore_send_records: bool,
    /// Allow missing TPID record
    #[arg(long)]
    ignore_missing_tpid: bool,
    /// Preserve original float field strings for byte-exact roundtrip
    #[arg(long)]
    preserve_value_strings: bool,
    /// Append line numbers to output
    #[arg(long)]
    include_linenum: bool,
    /// Use sign slot for extra precision on positive numbers
    #[arg(long)]
    abuse_signpos: bool,
    /// Drop leading zero in decimal form for extra precision
    #[arg(long)]
    skip_intzero: bool,
    /// Prefer decimal over scientific when it gives more precision
    #[arg(long)]
    prefer_noexp: bool,
    /// Keep the 'E' character in scientific notation
    #[arg(long)]
    keep_e: bool,
    /// Reject float values in integer fields even when integer-valued
    #[arg(long)]
    strict_datatypes: bool,
    /// Write zero fields as blanks in boundary records
    #[arg(long)]
    zero_as_blank: bool,
    /// Continue parsing on failure (store raw sections)
    #[arg(long)]
    nofail: bool,
    /// Shorthand: enable all ignore_*_mismatch options
    #[arg(long)]
    ignore_mismatches: bool,
}

// ---------------------------------------------------------------------------
// Parser construction from CommonOpts
// ---------------------------------------------------------------------------

fn build_parser(opts: &CommonOpts) -> EndfParser {
    let mut builder = EndfParser::builder();

    if let Some(ref dir) = opts.recipes_dir {
        builder = builder.recipes_dir(dir.as_str());
    } else {
        builder = builder.endf_format(&opts.endf_format);
    }

    let ignore_num = opts.ignore_number_mismatch || opts.ignore_mismatches;
    let ignore_zero = opts.ignore_zero_mismatch || opts.ignore_mismatches;
    let ignore_var = opts.ignore_varspec_mismatch || opts.ignore_mismatches;

    builder = builder
        .ignore_number_mismatch(ignore_num)
        .ignore_zero_mismatch(ignore_zero)
        .ignore_varspec_mismatch(ignore_var)
        .fuzzy_matching(opts.fuzzy_matching)
        .accept_spaces(opts.accept_spaces)
        .ignore_blank_lines(opts.ignore_blank_lines)
        .ignore_send_records(opts.ignore_send_records)
        .ignore_missing_tpid(opts.ignore_missing_tpid)
        .preserve_value_strings(opts.preserve_value_strings)
        .nofail(opts.nofail);

    // Write options (only applied when explicitly set; otherwise use defaults)
    if opts.include_linenum { builder = builder.include_linenum(true); }
    if opts.abuse_signpos { builder = builder.abuse_signpos(true); }
    if opts.skip_intzero { builder = builder.skip_intzero(true); }
    if opts.prefer_noexp { builder = builder.prefer_noexp(true); }
    if opts.keep_e { builder = builder.keep_e(true); }
    if opts.strict_datatypes { builder = builder.strict_datatypes(true); }
    if opts.zero_as_blank { builder = builder.zero_as_blank(true); }

    builder.build().unwrap_or_else(|e| {
        eprintln!("Error creating parser: {}", e);
        process::exit(1);
    })
}


// ---------------------------------------------------------------------------
// Parse file with optional parallel support
// ---------------------------------------------------------------------------

fn parse_file(parser: &EndfParser, path: &str, threads: usize) -> EndfValue {
    eprintln!("Parsing {}{}...", path,
        if threads != 1 { format!(" ({} threads)", threads) } else { String::new() });

    #[cfg(feature = "parallel")]
    if threads != 1 {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global();
    }

    #[cfg(feature = "parallel")]
    let result = if threads == 1 {
        parser.parse_file(Path::new(path))
    } else {
        parser.parse_file_parallel(Path::new(path))
    };
    #[cfg(not(feature = "parallel"))]
    let result = parser.parse_file(Path::new(path));

    result.unwrap_or_else(|e| {
        eprintln!("Parse error: {}", e);
        process::exit(1);
    })
}

// ---------------------------------------------------------------------------
// Subcommand implementations
// ---------------------------------------------------------------------------

fn cmd_endf2json(input: &str, output: &str, pretty: bool, lossless: bool,
                 threads: usize, common: &CommonOpts) {
    let parser = build_parser(common);
    let data = parse_file(&parser, input, threads);

    eprintln!("Writing {}...", output);
    let json_str = if lossless {
        if pretty {
            serde_json::to_string_pretty(&data)
        } else {
            serde_json::to_string(&data)
        }
    } else {
        json::to_json_string(&data, pretty)
    }.unwrap_or_else(|e| {
        eprintln!("JSON serialization error: {}", e);
        process::exit(1);
    });

    write_output(output, &json_str);
    eprintln!("Done.");
}

fn cmd_json2endf(input: &str, output: &str, _threads: usize, common: &CommonOpts) {
    let parser = build_parser(common);

    eprintln!("Reading {}...", input);
    let json_str = std::fs::read_to_string(input).unwrap_or_else(|e| {
        eprintln!("Read error: {}", e);
        process::exit(1);
    });

    let data: EndfValue = json::from_json_string(&json_str).or_else(|_| {
        serde_json::from_str::<EndfValue>(&json_str)
    }).unwrap_or_else(|e| {
        eprintln!("JSON parse error: {}", e);
        process::exit(1);
    });

    eprintln!("Writing {}...", output);
    let endf_text = parser.write(&data).unwrap_or_else(|e| {
        eprintln!("Write error: {}", e);
        process::exit(1);
    });

    write_output(output, &endf_text);
    eprintln!("Done.");
}

fn cmd_convert(source: &str, dest: &str, to: &str, indent: Option<usize>,
               lossless: bool, threads: usize, common: &CommonOpts) {
    if Path::new(dest).exists() {
        eprintln!("Error: destination file already exists: {}", dest);
        process::exit(1);
    }
    match to {
        "json" => {
            let pretty = indent.is_some();
            cmd_endf2json(source, dest, pretty, lossless, threads, common);
        }
        "endf" => {
            cmd_json2endf(source, dest, threads, common);
        }
        _ => unreachable!("clap validates --to values"),
    }
}

fn cmd_validate(files: &[String], common: &CommonOpts) {
    if files.is_empty() {
        eprintln!("No files specified.");
        process::exit(1);
    }

    let parser = build_parser(common);
    let mut any_failed = false;
    let mut results: Vec<(&str, &str)> = Vec::new();

    for file in files {
        match parser.parse_file(Path::new(file)) {
            Ok(_) => {
                results.push((file, "ok"));
            }
            Err(e) => {
                any_failed = true;
                results.push((file, "FAILED"));
                eprintln!("{}", "=".repeat(80));
                eprintln!("Validation of {} failed:", file);
                eprintln!("{}", e);
            }
        }
    }

    eprintln!();
    eprintln!("========== VALIDATION SUMMARY ==========");
    for (file, status) in &results {
        eprintln!("  {} - {}", status, file);
    }

    if any_failed {
        process::exit(1);
    }
}

fn cmd_compare(file1: &str, file2: &str, atol: f64, rtol: f64, common: &CommonOpts) {
    let parser = build_parser(common);
    let data1 = parse_file(&parser, file1, 1);
    let data2 = parse_file(&parser, file2, 1);

    let mut diffs: Vec<String> = Vec::new();
    compare_values(&data1, &data2, "", atol, rtol, &mut diffs);

    if diffs.is_empty() {
        eprintln!("Files are equal (within tolerance atol={}, rtol={}).", atol, rtol);
    } else {
        eprintln!("Files differ ({} differences found):", diffs.len());
        for (i, diff) in diffs.iter().enumerate() {
            if i >= 100 {
                eprintln!("  ... and {} more differences", diffs.len() - 100);
                break;
            }
            eprintln!("  {}", diff);
        }
        process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Comparison engine
// ---------------------------------------------------------------------------

fn floats_close(a: f64, b: f64, atol: f64, rtol: f64) -> bool {
    if a == b { return true; }
    (a - b).abs() <= atol + rtol * b.abs()
}

fn compare_values(
    a: &EndfValue, b: &EndfValue,
    path: &str, atol: f64, rtol: f64,
    diffs: &mut Vec<String>,
) {
    if diffs.len() > 1000 { return; }

    match (a, b) {
        (EndfValue::Int(x), EndfValue::Int(y)) => {
            if x != y {
                diffs.push(format!("{}: Int {} vs {}", path, x, y));
            }
        }
        (EndfValue::Float(x), EndfValue::Float(y))
        | (EndfValue::PreservedFloat(x, _), EndfValue::Float(y))
        | (EndfValue::Float(x), EndfValue::PreservedFloat(y, _))
        | (EndfValue::PreservedFloat(x, _), EndfValue::PreservedFloat(y, _)) => {
            if !floats_close(*x, *y, atol, rtol) {
                diffs.push(format!("{}: Float {} vs {}", path, x, y));
            }
        }
        (EndfValue::Int(x), EndfValue::Float(y))
        | (EndfValue::Float(y), EndfValue::Int(x))
        | (EndfValue::Int(x), EndfValue::PreservedFloat(y, _))
        | (EndfValue::PreservedFloat(y, _), EndfValue::Int(x)) => {
            if !floats_close(*x as f64, *y, atol, rtol) {
                diffs.push(format!("{}: {} vs {}", path, x, y));
            }
        }
        (EndfValue::Str(x), EndfValue::Str(y)) => {
            if x.trim_end() != y.trim_end() {
                diffs.push(format!("{}: Str {:?} vs {:?}", path, x.trim_end(), y.trim_end()));
            }
        }
        (EndfValue::Dict(da), EndfValue::Dict(db)) => {
            for key in da.keys() {
                if !db.contains_key(key) {
                    diffs.push(format!("{}/{}: only in first file", path, key));
                }
            }
            for key in db.keys() {
                if !da.contains_key(key) {
                    diffs.push(format!("{}/{}: only in second file", path, key));
                }
            }
            for (key, val_a) in da {
                if let Some(val_b) = db.get(key) {
                    let child = format!("{}/{}", path, key);
                    compare_values(val_a, val_b, &child, atol, rtol, diffs);
                }
            }
        }
        (EndfValue::List(la), EndfValue::List(lb)) => {
            if la.len() != lb.len() {
                diffs.push(format!("{}: List length {} vs {}", path, la.len(), lb.len()));
                return;
            }
            for (i, (ea, eb)) in la.iter().zip(lb.iter()).enumerate() {
                let child = format!("{}[{}]", path, i);
                match (ea, eb) {
                    (Some(va), Some(vb)) => compare_values(va, vb, &child, atol, rtol, diffs),
                    (None, None) => {}
                    _ => diffs.push(format!("{}: Some vs None", child)),
                }
            }
        }
        _ => {
            let tn = |v: &EndfValue| match v {
                EndfValue::Int(_) => "Int",
                EndfValue::Float(_) | EndfValue::PreservedFloat(_, _) => "Float",
                EndfValue::Str(_) => "Str",
                EndfValue::Dict(_) => "Dict",
                EndfValue::List(_) => "List",
            };
            diffs.push(format!("{}: type mismatch ({} vs {})", path, tn(a), tn(b)));
        }
    }
}

// ---------------------------------------------------------------------------
// Output helper
// ---------------------------------------------------------------------------

fn write_output(path: &str, content: &str) {
    if path == "-" {
        println!("{}", content);
    } else {
        std::fs::write(path, content).unwrap_or_else(|e| {
            eprintln!("Write error: {}", e);
            process::exit(1);
        });
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Endf2json { input, output, pretty, lossless, threads, common } => {
            cmd_endf2json(&input, &output, pretty, lossless, threads, &common);
        }
        Commands::Json2endf { input, output, threads, common } => {
            cmd_json2endf(&input, &output, threads, &common);
        }
        Commands::Convert { source, dest, to, indent, lossless, threads, common } => {
            cmd_convert(&source, &dest, &to, indent, lossless, threads, &common);
        }
        Commands::Validate { files, common } => {
            cmd_validate(&files, &common);
        }
        Commands::Compare { file1, file2, atol, rtol, common } => {
            cmd_compare(&file1, &file2, atol, rtol, &common);
        }
    }
}
