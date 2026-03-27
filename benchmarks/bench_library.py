"""Benchmark: parse the entire ENDF/B-VIII.1 neutron library.

Compares C++ compiled parser (endf-parserpy) vs Rust interpreter.
"""

import os
import sys
import time
import subprocess

ENDF_DIR = "/path/to/endf/library"

files = sorted(
    os.path.join(ENDF_DIR, f) for f in os.listdir(ENDF_DIR) if f.endswith(".endf")
)
total_mb = sum(os.path.getsize(f) for f in files) / 1024 / 1024
print(f"ENDF/B-VIII.1 neutron library: {len(files)} files, {total_mb:.0f} MB")
print()

# ── C++ compiled parser ──────────────────────────────────────────────
print("=" * 60)
print("C++ compiled parser (endf-parserpy EndfParserCpp)")
print("=" * 60)

from endf_parserpy import EndfParserCpp

cpp_parser = EndfParserCpp(
    ignore_number_mismatch=True,
    ignore_zero_mismatch=True,
    ignore_varspec_mismatch=True,
)

# Warmup
cpp_parser.parsefile(files[0])

t0 = time.perf_counter()
cpp_sections = 0
cpp_errors = 0
for f in files:
    try:
        data = cpp_parser.parsefile(f)
        cpp_sections += sum(len(mt) for mt in data.values())
    except Exception:
        cpp_errors += 1
t1 = time.perf_counter()
cpp_time = t1 - t0
print(f"  Time:     {cpp_time:.2f}s")
print(f"  Sections: {cpp_sections}")
print(f"  Errors:   {cpp_errors}")
print(f"  Rate:     {total_mb / cpp_time:.1f} MB/s")
print()

# ── Rust interpreter ─────────────────────────────────────────────────
print("=" * 60)
print("Rust interpreter (endf-parser)")
print("=" * 60)

RUST_DIR = os.path.join(os.path.dirname(__file__), "..")
BENCH_BIN = os.path.join(RUST_DIR, "target", "release", "bench_library")

# Build
print("  Building...")
result = subprocess.run(
    ["cargo", "build", "--release", "--bin", "bench_library", "-p", "endf-parser"],
    cwd=RUST_DIR,
    capture_output=True,
    text=True,
)
if result.returncode != 0:
    print(f"  Build failed: {result.stderr[-500:]}")
    sys.exit(1)

# Run single process for entire library
result = subprocess.run(
    [BENCH_BIN, ENDF_DIR],
    capture_output=True,
    text=True,
)
if result.returncode != 0:
    print(f"  Run failed: {result.stderr[-500:]}")
    sys.exit(1)

metrics = {}
for line in result.stdout.strip().split("\n"):
    k, v = line.split("=")
    metrics[k] = float(v) if "." in v else int(v)

rust_time = metrics["time"]
rust_sections = int(metrics["sections"])
rust_errors = int(metrics["errors"])
rust_rate = metrics["rate"]
print(f"  Time:     {rust_time:.2f}s")
print(f"  Sections: {rust_sections}")
print(f"  Errors:   {rust_errors}")
print(f"  Rate:     {rust_rate:.1f} MB/s")
print()

# ── Summary ──────────────────────────────────────────────────────────
print("=" * 60)
print("SUMMARY — Full ENDF/B-VIII.1 neutron library parse")
print("=" * 60)
print()
print(f"  {'Parser':<25} {'Time':>8} {'Sections':>10} {'Rate':>10}")
print(f"  {'-'*55}")
print(
    f"  {'C++ compiled':<25} {cpp_time:>7.1f}s {cpp_sections:>10} {total_mb/cpp_time:>8.1f} MB/s"
)
print(
    f"  {'Rust interpreter':<25} {rust_time:>7.1f}s {rust_sections:>10} {rust_rate:>8.1f} MB/s"
)
print()
print(f"  C++/Rust speed ratio: {rust_time / cpp_time:.1f}x")
print()
