#!/usr/bin/env python3
"""Benchmark: Python C++ parser vs Rust interpreter vs Compiled Rust parser.

All three are called via their Python APIs to ensure a fair comparison
(same I/O overhead, same Python loop, same file list).
"""

import glob
import os
import sys
import time

ENDF_DIR = "/path/to/endf/library"

PARSER_OPTS = dict(
    ignore_number_mismatch=True,
    ignore_zero_mismatch=True,
    ignore_varspec_mismatch=True,
    ignore_send_records=True,
    ignore_missing_tpid=True,
    ignore_blank_lines=True,
)


def benchmark(name, parse_fn, files, mb):
    """Run a single benchmark: warmup + timed run."""
    # Warmup
    try:
        parse_fn(files[0])
    except Exception:
        pass

    errors = 0
    t0 = time.perf_counter()
    for f in files:
        try:
            parse_fn(f)
        except Exception:
            errors += 1
    elapsed = time.perf_counter() - t0
    rate = mb / elapsed
    print(f"  {name:30s}  {elapsed:6.1f}s  {rate:6.1f} MB/s  errors={errors}")
    return elapsed, rate, errors


def main():
    endf_dir = sys.argv[1] if len(sys.argv) > 1 else ENDF_DIR
    files = sorted(glob.glob(os.path.join(endf_dir, "*.endf")))
    total_bytes = sum(os.path.getsize(f) for f in files)
    mb = total_bytes / 1024 / 1024

    print(f"Files: {len(files)}, Total: {mb:.1f} MB")
    print()

    # --- Python C++ parser ---
    try:
        from endf_parserpy import EndfParserCpp
        cpp = EndfParserCpp(**PARSER_OPTS, endf_format="endf6")
        benchmark("Python C++ parser (endf6)", lambda f: cpp.parsefile(f), files, mb)
    except ImportError:
        print("  Python C++ parser: not available")

    # --- Rust interpreter ---
    try:
        from endf_parser_py import EndfParser
        interp = EndfParser(**PARSER_OPTS)
        benchmark("Rust interpreter", lambda f: interp.parsefile(f), files, mb)
    except ImportError:
        print("  Rust interpreter: not available")

    # --- Compiled Rust parser ---
    try:
        from endf_parser_py import CompiledParser
        compiled = CompiledParser(**PARSER_OPTS)
        benchmark("Compiled Rust parser", lambda f: compiled.parsefile(f), files, mb)
    except ImportError:
        print("  Compiled Rust parser: not available")

    print()


if __name__ == "__main__":
    main()
