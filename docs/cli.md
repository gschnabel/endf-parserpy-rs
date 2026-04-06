# endf-tool CLI Reference

`endf-tool` is a command-line utility for working with ENDF-6 nuclear data
files. It can convert between ENDF and JSON formats, validate files, compare
files, inspect and modify file contents, and search across multiple files
using query expressions.

## Installation

```bash
cargo install --path crates/endf
# or:
cargo build --release --bin endf_tool
```

The binary is `endf_tool` (or `endf-tool` depending on your shell alias).

## Commands

- [convert](#convert) -- Convert between ENDF and JSON
- [endf2json / json2endf](#endf2json--json2endf) -- Direct conversion shortcuts
- [validate](#validate) -- Validate ENDF files for compliance
- [compare](#compare) -- Compare two ENDF files
- [show](#show) -- Inspect a section or variable
- [match](#match) -- Search files using query expressions
- [replace](#replace) -- Replace a section or variable across files
- [insert-text](#insert-text) -- Insert description text into MF1/MT451
- [update-directory](#update-directory) -- Regenerate MF1/MT451 directory

## Common Options

All commands accept these parser and writer options:

### Parsing options

| Flag | Description |
|------|-------------|
| `--endf-format <FMT>` | Recipe format: `endf6`, `endf6-ext` (default), `jendl`, `pendf`, `errorr` |
| `--recipes-dir <DIR>` | Load recipes from a directory (overrides `--endf-format`) |
| `--ignore-number-mismatch` | Tolerate mismatches between field values and recipe constants |
| `--ignore-zero-mismatch` | Tolerate non-zero values in zero-expected fields |
| `--ignore-varspec-mismatch` | Tolerate inconsistent variable assignments across records |
| `--fuzzy-matching` | Use approximate comparison (atol=1e-7, rtol=1e-5) for field validation |
| `--accept-spaces` | Strip spaces inside numeric fields before parsing |
| `--ignore-blank-lines` | Skip blank lines without error |
| `--ignore-send-records` | Skip SEND/FEND/MEND/TEND boundary record validation |
| `--ignore-missing-tpid` | Allow files without a TPID (tape identification) header |
| `--preserve-value-strings` | Preserve original float field strings for byte-exact roundtrip |
| `--nofail` | Continue parsing on failure (store failed sections as raw text) |
| `--ignore-mismatches` | Shorthand: enables `--ignore-number-mismatch`, `--ignore-zero-mismatch`, and `--ignore-varspec-mismatch` |

### Writing options

| Flag | Description |
|------|-------------|
| `--include-linenum` | Append 5-digit line numbers to each output line |
| `--abuse-signpos` | Use the sign slot for an extra mantissa digit on positive numbers |
| `--skip-intzero` | Drop leading zero in decimal form (`.12345` instead of `0.12345`) |
| `--prefer-noexp` | Prefer decimal over scientific notation when it gives more precision |
| `--keep-e` | Include the `E` character in scientific notation (`1.23E+8` vs `1.23+8`) |
| `--strict-datatypes` | Reject float-typed values in integer fields, even if integer-valued |
| `--zero-as-blank` | Write zero fields as blanks in SEND/FEND/MEND/TEND records |

---

## convert

Convert between ENDF and JSON formats.

```bash
endf-tool convert <SOURCE> <DEST> --to <FORMAT> [OPTIONS]
```

**Arguments:**
- `SOURCE` -- Input file path
- `DEST` -- Output file path (must not already exist)
- `--to endf|json` -- Destination format (required)

**Options:**
- `--indent <N>` -- JSON indentation level (implies pretty-print)
- `--lossless` -- Use tagged JSON format that preserves Int/Float type distinctions
- `--threads <N>` -- Parallel parsing threads (0=auto, 1=sequential; default: 1)

**Examples:**

```bash
# ENDF to pretty JSON
endf-tool convert input.endf output.json --to json --indent 2

# JSON back to ENDF
endf-tool convert output.json roundtrip.endf --to endf

# With tolerance flags for real-world files
endf-tool convert input.endf output.json --to json --ignore-mismatches --ignore-send-records
```

---

## endf2json / json2endf

Direct conversion shortcuts (same functionality as `convert`).

```bash
endf-tool endf2json <INPUT> <OUTPUT> [--pretty] [--lossless] [--threads N]
endf-tool json2endf <INPUT> <OUTPUT>
```

Use `-` as `OUTPUT` to write to stdout.

**Examples:**

```bash
# Pretty-print to stdout
endf-tool endf2json input.endf - --pretty --ignore-mismatches

# Parallel parsing (all CPUs)
endf-tool endf2json input.endf output.json --threads 0
```

---

## validate

Validate one or more ENDF files for compliance with the recipe format.
Validation means attempting to parse each file; a file is "valid" if
parsing succeeds without error.

```bash
endf-tool validate <FILE>... [OPTIONS]
```

**Arguments:**
- `FILE` -- One or more ENDF files (glob patterns supported)

By default, all `--ignore-*` flags are `false` (strict mode). Pass
individual flags to relax specific checks.

**Exit codes:**
- `0` -- All files valid
- `1` -- At least one file failed

**Examples:**

```bash
# Strict validation
endf-tool validate data/*.endf

# Relaxed validation (tolerate common deviations)
endf-tool validate data/*.endf --ignore-zero-mismatch --accept-spaces
```

**Output:**

```
================================================================================
Validation of bad_file.endf failed:
number mismatch in field 'N2': expected 0, got 1
...

========== VALIDATION SUMMARY ==========
  ok - good_file.endf
  FAILED - bad_file.endf
```

---

## compare

Compare two ENDF files at the parsed data level with configurable
float tolerances.

```bash
endf-tool compare <FILE1> <FILE2> [--atol <TOL>] [--rtol <TOL>]
```

**Arguments:**
- `FILE1`, `FILE2` -- ENDF files to compare

**Options:**
- `--atol <TOL>` -- Absolute tolerance for float comparison (default: 1e-8)
- `--rtol <TOL>` -- Relative tolerance for float comparison (default: 1e-6)

Comparison uses: `|a - b| <= atol + rtol * |b|`

**Exit codes:**
- `0` -- Files are equal (within tolerance)
- `1` -- Files differ

**Examples:**

```bash
# Compare with default tolerances
endf-tool compare original.endf modified.endf --ignore-mismatches

# Tight comparison
endf-tool compare file1.endf file2.endf --atol 0 --rtol 0

# Loose comparison
endf-tool compare file1.endf file2.endf --atol 1e-3 --rtol 1e-3
```

**Output (when files differ):**

```
Files differ (42 differences found):
  /1/451/ZA: Float 26056 vs 26057
  /3/1/xstable/E[0]: Float 1e-5 vs 1.1e-5
  ...
```

---

## show

Display the contents of a section or variable from an ENDF file,
addressed by an EndfPath.

```bash
endf-tool show <ENDFPATH> <FILE>
```

**Arguments:**
- `ENDFPATH` -- Slash-separated path (e.g., `1/451`, `3/1/xstable/E`, `1/451/ZA`)
- `FILE` -- ENDF file to inspect

Only the section(s) addressed by the first two path elements are
parsed (for efficiency).

**Output format:**
- Scalars: the value is printed directly
- Containers: one level of keys is listed with their values or
  `<subsection or array>` for nested structures

**Examples:**

```bash
# List all variables in MF1/MT451
endf-tool show 1/451 file.endf --ignore-mismatches

# Get a specific value
endf-tool show 1/451/ZA file.endf --ignore-mismatches
# Output: 26056

# List xstable contents
endf-tool show 3/1/xstable file.endf --ignore-mismatches
# Output:
# /NBT   <subsection or array>
# /INT   <subsection or array>
# /E     <subsection or array>
# /xs    <subsection or array>
```

---

## match

Search ENDF files using logical query expressions.

```bash
endf-tool match <FILE>... -q "<QUERY>"
```

**Arguments:**
- `FILE` -- One or more ENDF files (glob patterns supported)
- `-q, --query <QUERY>` -- Query expression

### Query Language

| Syntax | Meaning | Example |
|--------|---------|---------|
| `/path > N` | Numeric comparison | `/1/451/ZA > 92000` |
| `/path == N` | Equality | `/3/1/NR == 1` |
| `expr & expr` | Logical AND | `/1/451/ZA > 26000 & /3/1/NR == 1` |
| `expr \| expr` | Logical OR | `/1/451/ZA > 92000 \| /1/451/ZA < 1000` |
| `!(expr)` | Negation | `!(/1/451/LFI == 0)` |
| `exists(/path)` | Path exists | `exists(/8/457)` |
| `/path/*/field` | Wildcard | `/3/*/NR > 0` |

Comparison operators: `==`, `!=`, `<`, `>`, `<=`, `>=`

Wildcards (`*`) expand over all keys at that level. The query is
true if *any* expansion satisfies the condition.

**Exit codes:**
- `0` -- Success (matches may or may not have been found)
- `1` -- Parse error on at least one file

**Examples:**

```bash
# Find all uranium isotopes
endf-tool match data/*.endf -q "/1/451/ZA > 92000" --ignore-mismatches

# Find files with fission data (MF=18 exists)
endf-tool match data/*.endf -q "exists(/3/18)" --ignore-mismatches

# Complex query
endf-tool match data/*.endf -q "/1/451/ZA > 26000 & exists(/8/457)" --ignore-mismatches
```

**Output:**

```
match: data/n-092_U_235.endf
  /1/451/ZA = 92235
match: data/n-092_U_238.endf
  /1/451/ZA = 92238
```

---

## replace

Replace a section or variable in one or more destination ENDF files
with data from a source file.

```bash
endf-tool replace <ENDFPATH> <SOURCE> <DEST>... [-n]
```

**Arguments:**
- `ENDFPATH` -- Path to the element to replace (e.g., `1/451/ZA`, `3/1`, `1`)
- `SOURCE` -- Source file to extract the replacement value from
- `DEST` -- One or more destination files (glob patterns supported)

**Options:**
- `-n, --no-backup` -- Disable `.bak` backup file creation

By default, the original destination file is renamed to `file.bak`
before writing the modified version. Use `-n` to skip the backup.

For sub-section replacement (path depth > 2), `--preserve-value-strings`
is automatically enabled to preserve float formatting in untouched fields.

**Examples:**

```bash
# Replace ZA in destination with value from source
endf-tool replace 1/451/ZA source.endf dest.endf --ignore-mismatches

# Replace entire MF3 section
endf-tool replace 3 source.endf dest.endf --ignore-mismatches

# Replace across multiple files (no backup)
endf-tool replace 1/451/ZA source.endf data/*.endf -n --ignore-mismatches
```

---

## insert-text

Insert description text into the MF1/MT451 DESCRIPTION field.
Text is read from standard input.

```bash
echo "My description line" | endf-tool insert-text <FILE> [-l N] [-n]
```

**Arguments:**
- `FILE` -- ENDF file to modify

**Options:**
- `-l, --line <N>` -- Insert after this line number (default: 0 = beginning)
- `-n, --no-backup` -- Disable `.bak` backup

After insertion, NWD (number of description words) is updated
automatically, and the MF1/MT451 directory is regenerated to reflect
the new line counts.

**Examples:**

```bash
# Insert at the beginning of the description
echo "Modified by endf-tool on 2026-04-06" | endf-tool insert-text file.endf --ignore-mismatches

# Insert after line 5
echo "New line" | endf-tool insert-text file.endf -l 5 --ignore-mismatches

# Multi-line insertion
cat notes.txt | endf-tool insert-text file.endf --ignore-mismatches
```

---

## update-directory

Regenerate the MF1/MT451 directory listing. The directory is the index
of all MF/MT sections in the file, including their line counts.

```bash
endf-tool update-directory <FILE> [-n]
```

**Arguments:**
- `FILE` -- ENDF file to modify

**Options:**
- `-n, --no-backup` -- Disable `.bak` backup

This command serializes the entire file to count lines per section,
then updates the NXC, MFx, MTx, NCx, and MOD arrays in MF1/MT451.
Existing MOD (modification flag) values are preserved.

**Examples:**

```bash
# Update directory after manual edits
endf-tool update-directory file.endf --ignore-mismatches

# Without backup
endf-tool update-directory file.endf -n --ignore-mismatches
```

---

## EndfPath Syntax

Several commands use EndfPath notation to address elements in the
parsed ENDF data structure:

| Path | Target |
|------|--------|
| `1` | All of MF=1 |
| `1/451` | MF=1, MT=451 section |
| `1/451/ZA` | The ZA variable in MF1/MT451 |
| `3/1/xstable/E` | The energy array in MF3/MT1's xstable |
| `2/151/isotope[1]` | First isotope subsection in MF2/MT151 |
| `3/*/NR` | NR from all MTs under MF=3 (wildcard) |

Path elements:
- **Integers** (`1`, `451`) -- MF/MT numbers or array indices
- **Strings** (`ZA`, `xstable`) -- Variable names
- **`*`** -- Wildcard (iterates over all keys at that level)
- **Brackets** (`isotope[1,2]`) -- Shorthand for `isotope/1/2`
