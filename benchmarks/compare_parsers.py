#!/usr/bin/env python3
"""Compare Python C++ parser vs Rust interpreter on ENDF/B-VIII.1 library.

Phase 1: Roundtrip test — parse every file with each parser, report errors.
Phase 2: Structural comparison — compare the nested dict layout of both parsers.
"""

import os
import sys
import glob
import traceback
from collections import defaultdict


ENDF_DIR = "/path/to/endf/library"

PARSER_OPTS = dict(
    ignore_number_mismatch=True,
    ignore_zero_mismatch=True,
    ignore_varspec_mismatch=True,
    ignore_send_records=True,
    ignore_missing_tpid=True,
    ignore_blank_lines=True,
)


def make_cpp_parser():
    from endf_parserpy import EndfParserCpp
    return EndfParserCpp(**PARSER_OPTS, endf_format="endf6")


def make_rust_parser():
    from endf_parser_py import EndfParser
    return EndfParser(**PARSER_OPTS)


def compare_values(a, b, path, diffs, max_diffs=5):
    """Recursively compare two Python values, collecting differences."""
    if len(diffs) >= max_diffs:
        return

    ta, tb = type(a).__name__, type(b).__name__

    if isinstance(a, dict) and isinstance(b, dict):
        keys_a = set(a.keys())
        keys_b = set(b.keys())
        for k in sorted(keys_a - keys_b, key=str):
            diffs.append(f"{path}: key {k!r} missing in Rust")
        for k in sorted(keys_b - keys_a, key=str):
            diffs.append(f"{path}: key {k!r} extra in Rust")
        for k in sorted(keys_a & keys_b, key=str):
            compare_values(a[k], b[k], f"{path}/{k}", diffs, max_diffs)

    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            diffs.append(f"{path}: list len {len(a)} (C++) vs {len(b)} (Rust)")
        for i in range(min(len(a), len(b))):
            compare_values(a[i], b[i], f"{path}[{i}]", diffs, max_diffs)

    elif isinstance(a, (int, float)) and isinstance(b, (int, float)):
        fa, fb = float(a), float(b)
        if abs(fa - fb) > 1e-10 + 1e-10 * abs(fb):
            diffs.append(f"{path}: {a} (C++) vs {b} (Rust)")

    elif isinstance(a, str) and isinstance(b, str):
        if a.strip() != b.strip():
            diffs.append(f"{path}: str differs: {a!r} vs {b!r}")

    elif a is None and b is None:
        pass

    else:
        diffs.append(f"{path}: type {ta} (C++) vs {tb} (Rust)")


def run_tests(endf_dir, single_file=None, max_files=None):
    cpp_parser = make_cpp_parser()
    rust_parser = make_rust_parser()

    if single_file:
        files = [single_file]
    else:
        files = sorted(glob.glob(os.path.join(endf_dir, "*.endf")))
    if max_files:
        files = files[:max_files]

    print(f"Testing {len(files)} ENDF files\n")

    # Phase 1: Roundtrip tests
    print("=" * 60)
    print("PHASE 1: Roundtrip parsing")
    print("=" * 60)

    cpp_ok, cpp_fail = 0, []
    rust_ok, rust_fail = 0, []
    cpp_results = {}
    rust_results = {}

    for fpath in files:
        fname = os.path.basename(fpath)

        # C++ parser
        try:
            cpp_data = cpp_parser.parsefile(fpath)
            cpp_ok += 1
            cpp_results[fname] = cpp_data
        except Exception as e:
            cpp_fail.append((fname, str(e)[:200]))

        # Rust parser
        try:
            rust_data = rust_parser.parsefile(fpath)
            rust_ok += 1
            rust_results[fname] = rust_data
        except Exception as e:
            rust_fail.append((fname, str(e)[:200]))

    print(f"\nC++ parser:  {cpp_ok} OK, {len(cpp_fail)} FAILED out of {len(files)}")
    if cpp_fail:
        print("  Failures:")
        for fname, err in cpp_fail[:10]:
            print(f"    {fname}: {err}")
        if len(cpp_fail) > 10:
            print(f"    ... and {len(cpp_fail) - 10} more")

    print(f"Rust parser: {rust_ok} OK, {len(rust_fail)} FAILED out of {len(files)}")
    if rust_fail:
        print("  Failures:")
        for fname, err in rust_fail[:10]:
            print(f"    {fname}: {err}")
        if len(rust_fail) > 10:
            print(f"    ... and {len(rust_fail) - 10} more")

    # Phase 2: Structural comparison on files that both parsed successfully
    print(f"\n{'=' * 60}")
    print("PHASE 2: Structural comparison (C++ vs Rust)")
    print("=" * 60)

    common_files = sorted(set(cpp_results.keys()) & set(rust_results.keys()))
    print(f"\nComparing {len(common_files)} files parsed by both\n")

    total_sections = 0
    match_sections = 0
    diff_sections = 0
    diffs_by_mf = defaultdict(int)
    diff_details = []  # (fname, mf, mt, diffs)

    for fname in common_files:
        cpp_data = cpp_results[fname]
        rust_data = rust_results[fname]

        # Compare MF-level keys
        cpp_mfs = set(cpp_data.keys())
        rust_mfs = set(rust_data.keys())

        for mf in sorted(cpp_mfs | rust_mfs):
            if mf not in cpp_data or mf not in rust_data:
                continue

            cpp_mf = cpp_data[mf]
            rust_mf = rust_data[mf]

            if not isinstance(cpp_mf, dict) or not isinstance(rust_mf, dict):
                continue

            for mt in sorted(set(cpp_mf.keys()) | set(rust_mf.keys())):
                if mt not in cpp_mf or mt not in rust_mf:
                    continue

                cpp_sec = cpp_mf[mt]
                rust_sec = rust_mf[mt]

                # Skip unparsed sections (stored as lists of strings)
                if not isinstance(cpp_sec, dict) or not isinstance(rust_sec, dict):
                    continue

                total_sections += 1
                diffs = []
                compare_values(cpp_sec, rust_sec, "", diffs, max_diffs=10)

                if not diffs:
                    match_sections += 1
                else:
                    diff_sections += 1
                    diffs_by_mf[mf] += 1
                    if len(diff_details) < 200:
                        diff_details.append((fname, mf, mt, diffs))

    print(f"Total sections compared: {total_sections}")
    print(f"Matching:               {match_sections}")
    print(f"Differing:              {diff_sections}")

    if diffs_by_mf:
        print(f"\nDiffs by MF:")
        for mf in sorted(diffs_by_mf):
            print(f"  MF{mf}: {diffs_by_mf[mf]}")

    if diff_details:
        print(f"\nFirst {min(len(diff_details), 50)} diff details:")
        for fname, mf, mt, diffs in diff_details[:50]:
            print(f"  {fname} MF{mf}/MT{mt}:")
            for d in diffs[:5]:
                print(f"    {d}")
            if len(diffs) > 5:
                print(f"    ... ({len(diffs)} total diffs)")


if __name__ == "__main__":
    single = sys.argv[1] if len(sys.argv) > 1 else None
    max_f = int(sys.argv[2]) if len(sys.argv) > 2 else None
    if single and os.path.isdir(single):
        run_tests(single, max_files=max_f)
    elif single and os.path.isfile(single):
        run_tests(ENDF_DIR, single_file=single, max_files=max_f)
    else:
        run_tests(ENDF_DIR, max_files=max_f)
