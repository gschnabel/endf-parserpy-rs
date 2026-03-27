"""Benchmark: Python interpreter vs C++ compiled vs Rust parser.

Usage:
    python3 benchmarks/bench_parse.py [endf_file]

Defaults to Cu-63 test file if no argument given.
"""

import sys, os, time, statistics

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
DEFAULT_FILE = os.path.join(
    os.path.dirname(__file__),
    "..",
    "..",
    "tests",
    "testdata",
    "n_2925_29-Cu-63.endf",
)

ENDF_FILE = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_FILE
ENDF_FILE = os.path.abspath(ENDF_FILE)

if not os.path.exists(ENDF_FILE):
    print(f"File not found: {ENDF_FILE}")
    sys.exit(1)

# Number of repetitions for timing
WARMUP = 1
REPEATS = 5

print(f"Benchmark file: {ENDF_FILE}")
print(f"File size: {os.path.getsize(ENDF_FILE) / 1024:.0f} KB")
print(f"Warmup: {WARMUP}, Repeats: {REPEATS}")
print()


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def bench(name, parse_fn, write_fn=None):
    """Run parse (and optionally write) benchmarks, return median times."""
    # Warmup
    for _ in range(WARMUP):
        data = parse_fn()

    # Parse benchmark
    parse_times = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        data = parse_fn()
        t1 = time.perf_counter()
        parse_times.append(t1 - t0)

    med_parse = statistics.median(parse_times)
    print(
        f"  {name} parse:  {med_parse:.4f}s  (median of {REPEATS},"
        f" min={min(parse_times):.4f}s, max={max(parse_times):.4f}s)"
    )

    # Write benchmark
    if write_fn is not None:
        # Warmup
        for _ in range(WARMUP):
            write_fn(data)
        write_times = []
        for _ in range(REPEATS):
            t0 = time.perf_counter()
            write_fn(data)
            t1 = time.perf_counter()
            write_times.append(t1 - t0)
        med_write = statistics.median(write_times)
        print(
            f"  {name} write:  {med_write:.4f}s  (median of {REPEATS},"
            f" min={min(write_times):.4f}s, max={max(write_times):.4f}s)"
        )
    else:
        med_write = None

    return med_parse, med_write


# ---------------------------------------------------------------------------
# 1. Python interpreter (endf_parserpy.EndfParser)
# ---------------------------------------------------------------------------
print("=" * 60)
print("1. Python interpreter (endf_parserpy.EndfParser)")
print("=" * 60)

from endf_parserpy import EndfParser as PyParser

py_parser = PyParser(
    ignore_number_mismatch=True,
    ignore_zero_mismatch=True,
    ignore_varspec_mismatch=True,
    print_cache_info=False,
)

py_parse_time, py_write_time = bench(
    "Python",
    lambda: py_parser.parsefile(ENDF_FILE),
    lambda data: py_parser.write(data),
)

# Count sections
py_data = py_parser.parsefile(ENDF_FILE)
n_sections = sum(len(mt) for mt in py_data.values())
print(f"  Sections parsed: {n_sections}")
print()

# ---------------------------------------------------------------------------
# 2. C++ compiled parser (endf_parserpy.EndfParserCpp)
# ---------------------------------------------------------------------------
print("=" * 60)
print("2. C++ compiled parser (endf_parserpy.EndfParserCpp)")
print("=" * 60)

try:
    from endf_parserpy import EndfParserCpp

    cpp_parser = EndfParserCpp(
        ignore_number_mismatch=True,
        ignore_zero_mismatch=True,
        ignore_varspec_mismatch=True,
    )

    cpp_parse_time, cpp_write_time = bench(
        "C++",
        lambda: cpp_parser.parsefile(ENDF_FILE),
        lambda data: cpp_parser.write(data),
    )
    print()
except Exception as e:
    print(f"  C++ parser not available: {e}")
    cpp_parse_time = cpp_write_time = None
    print()

# ---------------------------------------------------------------------------
# 3. Rust parser (via subprocess — compiled binary)
# ---------------------------------------------------------------------------
print("=" * 60)
print("3. Rust parser (endf-parser, via benchmark binary)")
print("=" * 60)

import subprocess

# Build the benchmark binary
RUST_DIR = os.path.join(os.path.dirname(__file__), "..")
BENCH_BIN = os.path.join(RUST_DIR, "target", "release", "bench_parse")
BENCH_SRC = os.path.join(
    RUST_DIR, "crates", "endf-parser", "src", "bin", "bench_parse.rs"
)

# Create the benchmark binary source
os.makedirs(os.path.dirname(BENCH_SRC), exist_ok=True)
with open(BENCH_SRC, "w") as f:
    f.write(
        """
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
"""
    )

# Build
print("  Building Rust benchmark binary...")
result = subprocess.run(
    ["cargo", "build", "--release", "--bin", "bench_parse", "-p", "endf-parser"],
    cwd=RUST_DIR,
    capture_output=True,
    text=True,
)
if result.returncode != 0:
    print(f"  Build failed: {result.stderr}")
    rust_parse_time = rust_write_time = None
else:
    # Run
    result = subprocess.run(
        [BENCH_BIN, ENDF_FILE, str(REPEATS)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(f"  Run failed: {result.stderr}")
        rust_parse_time = rust_write_time = None
    else:
        metrics = {}
        for line in result.stdout.strip().split("\n"):
            k, v = line.split("=")
            metrics[k] = float(v) if "." in v else int(v)

        rust_parse_time = metrics["parse_median"]
        rust_write_time = metrics["write_median"]
        print(
            f"  Rust parse:   {rust_parse_time:.4f}s  (median of {REPEATS},"
            f" min={metrics['parse_min']:.4f}s, max={metrics['parse_max']:.4f}s)"
        )
        print(
            f"  Rust write:   {rust_write_time:.4f}s  (median of {REPEATS},"
            f" min={metrics['write_min']:.4f}s, max={metrics['write_max']:.4f}s)"
        )
        print(f"  Sections parsed: {metrics['sections']}")

print()

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
print("=" * 60)
print("SUMMARY")
print("=" * 60)
print()
print(f"{'Parser':<20} {'Parse (s)':>12} {'Write (s)':>12} {'Parse speedup':>15}")
print("-" * 60)


def fmt(t):
    return f"{t:.4f}" if t is not None else "N/A"


def speedup(ref_t, t):
    if ref_t is not None and t is not None and t > 0:
        return f"{ref_t / t:.1f}x"
    return "N/A"


print(
    f"{'Python interp.':<20} {fmt(py_parse_time):>12} {fmt(py_write_time):>12} {'1.0x (baseline)':>15}"
)
if cpp_parse_time is not None:
    print(
        f"{'C++ compiled':<20} {fmt(cpp_parse_time):>12} {fmt(cpp_write_time):>12} {speedup(py_parse_time, cpp_parse_time):>15}"
    )
if rust_parse_time is not None:
    print(
        f"{'Rust interpreter':<20} {fmt(rust_parse_time):>12} {fmt(rust_write_time):>12} {speedup(py_parse_time, rust_parse_time):>15}"
    )

print()
if rust_parse_time is not None and cpp_parse_time is not None:
    print(
        f"Rust vs C++: parse {speedup(cpp_parse_time, rust_parse_time)}, write {speedup(cpp_write_time, rust_write_time)}"
    )
