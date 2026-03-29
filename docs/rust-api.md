# endf-parser — Rust API Documentation

A recipe-driven toolkit for reading, writing, and modifying ENDF-6 nuclear
data files. Parses ENDF files into structured data using a formal grammar of
ENDF recipes, and writes them back with lossless roundtrip fidelity.

## Quick Start

```rust
use endf_parser::parser::EndfParser;
use endf_parser::value::{EndfValue, EndfKey};
use std::path::Path;

// Parse an ENDF file
let parser = EndfParser::new()?;
let data = parser.parse_file(Path::new("neutrons.endf"))?;

// Access data: MF=3, MT=1, variable "QM"
if let Some(qm) = data.get_path("3/1/QM") {
    println!("QM = {}", qm);
}

// Modify a value
let mut data = data;
data.set_path("3/1/QM", EndfValue::Float(1.5));

// Write back to ENDF format
let output = parser.write(&data)?;
std::fs::write("modified.endf", output)?;
```

---

## Core Types

### `EndfParser`

**Module:** `endf_parser::parser`

The main entry point. Holds a recipe catalogue and formatting options.
Create one via `EndfParser::new()` for defaults, or via the builder for
custom configuration.

```rust
use endf_parser::parser::EndfParser;

let parser = EndfParser::new()?;                       // defaults
let parser = EndfParser::builder()                     // custom
    .ignore_number_mismatch(true)
    .width(11)
    .build()?;
```

#### Methods

| Method | Description |
|--------|-------------|
| `new() -> EndfResult<Self>` | Create with default settings (ENDF-6 format). |
| `builder() -> EndfParserBuilder` | Create a configuration builder. |
| `parse(&self, input: &str) -> EndfResult<EndfValue>` | Parse ENDF text into a nested dictionary. |
| `parse_file(&self, path: &Path) -> EndfResult<EndfValue>` | Parse an ENDF file from disk. |
| `write(&self, data: &EndfValue) -> EndfResult<String>` | Write structured data back to ENDF-6 text. |
| `write_file(&self, path: &Path, data: &EndfValue) -> EndfResult<()>` | Write to a file on disk. |

---

### `EndfParserBuilder`

**Module:** `endf_parser::parser`

Fluent builder for `EndfParser`. All methods return `Self` and can be chained.

```rust
let parser = EndfParser::builder()
    .endf_format("endf6")           // recipe flavour
    .ignore_number_mismatch(true)   // tolerate DesiredNumber mismatches
    .ignore_zero_mismatch(true)     // tolerate non-zero where 0 expected
    .abuse_signpos(true)            // extra precision for positive numbers
    .prefer_noexp(true)             // prefer fixed-point when possible
    .include_linenum(true)          // add 5-digit line numbers
    .nofail(true)                   // continue past section errors
    .build()?;
```

#### Parse Options (recipe interpreter behaviour)

| Method | Default | Description |
|--------|---------|-------------|
| `ignore_number_mismatch(bool)` | `false` | Tolerate mismatches between ENDF data and recipe-expected numbers marked with `?` (DesiredNumber). |
| `ignore_zero_mismatch(bool)` | `true` | Tolerate non-zero values where the recipe specifies `0`. |
| `ignore_varspec_mismatch(bool)` | `true` | Tolerate mismatches for variables marked with `?` (inconsistent variable spec). |
| `fuzzy_matching(bool)` | `false` | Use floating-point tolerance instead of exact equality for validation. |
| `array_type(ArrayType)` | `Dict` | Store arrays as ordered dictionaries (`Dict`) or dense lists (`List`). |

#### Read Options (input format handling)

| Method | Default | Description |
|--------|---------|-------------|
| `accept_spaces(bool)` | `true` | Remove internal spaces from number fields (e.g., `"1.5 +02"` → `"1.5+02"`). |
| `preserve_value_strings(bool)` | `false` | Reserved for future EndfFloat integration. |
| `ignore_blank_lines(bool)` | `false` | Skip blank lines instead of erroring. |
| `ignore_send_records(bool)` | `false` | Don't validate SEND/FEND/MEND/TEND records. |
| `ignore_missing_tpid(bool)` | `false` | Allow files without a tape-header (TPID) record. |
| `width(usize)` | `11` | Field width in characters (applies to both read and write). |
| `nofail(bool)` | `false` | When `true`, store failed sections as raw strings instead of returning an error. |

#### Write Options (output formatting)

| Method | Default | Description |
|--------|---------|-------------|
| `abuse_signpos(bool)` | `true` | Use the sign position for an extra digit on positive numbers (7 significant digits instead of 6). |
| `skip_intzero(bool)` | `true` | Omit leading zero in decimals: `0.12` → `.12`. |
| `prefer_noexp(bool)` | `true` | Use fixed-point notation when it is at least as precise as scientific. |
| `keep_e(bool)` | `false` | Keep the `E` in scientific notation. When `false` (default), writes `1.23+7` instead of `1.23E+7`. |
| `include_linenum(bool)` | `true` | Append 5-digit line numbers (wrapping at 99999). |
| `strict_datatypes(bool)` | `false` | Error if a float value cannot be exactly converted to integer when an integer is required. |
| `zero_as_blank(bool)` | `false` | Write zero fields as blanks in SEND/FEND/MEND/TEND records. |

#### Recipe Selection

| Method | Default | Description |
|--------|---------|-------------|
| `endf_format(&str)` | `"endf6"` | Select recipe flavour: `"endf6"`, `"endf6-ext"`, `"jendl"`, `"pendf"`, `"errorr"`. |
| `recipes_dir(impl Into<PathBuf>)` | *none* | Load recipes from a directory at runtime (overrides `endf_format`). |

---

### `EndfValue`

**Module:** `endf_parser::value`

The dynamic value type representing all ENDF data. ENDF data is inherently
heterogeneous — the same structure can contain integers, floats, strings,
nested dictionaries, arrays, and interpolation tables.

#### Variants

| Variant | Rust Type | Description |
|---------|-----------|-------------|
| `Int(i64)` | integer | Control fields, counters, flags (MAT, MF, MT, NR, NP, ...) |
| `Float(f64)` | float | Physical quantities (cross sections, energies, ...) |
| `Str(String)` | string | TEXT record content, or unparsed raw section lines |
| `Dict(IndexMap<EndfKey, EndfValue>)` | ordered map | Sections, variable collections, MF/MT containers |
| `List(Vec<Option<EndfValue>>)` | dense list | Arrays in list mode (with `None` for gaps) |
| `Table(EndfTable)` | table struct | TAB1/TAB2 interpolation tables |

#### Data Structure

The `parse()` method returns a nested dictionary:

```
EndfValue::Dict {
    EndfKey::Int(mf) => EndfValue::Dict {       // MF level
        EndfKey::Int(mt) => EndfValue::Dict {    // MT level (section)
            EndfKey::Str("MAT")  => EndfValue::Int(2925),
            EndfKey::Str("ZA")   => EndfValue::Float(29063.0),
            EndfKey::Str("AWR")  => EndfValue::Float(62.389),
            EndfKey::Str("xstable") => EndfValue::Dict {    // named subsection
                EndfKey::Str("NBT") => EndfValue::List([...]),
                EndfKey::Str("INT") => EndfValue::List([...]),
                EndfKey::Str("E")   => EndfValue::List([...]),
                EndfKey::Str("xs")  => EndfValue::List([...]),
            },
            ...
        }
    }
}
```

#### Constructors

```rust
EndfValue::new_dict()        // empty dictionary
EndfValue::new_list()        // empty list
EndfValue::from(42_i64)      // Int
EndfValue::from(3.14_f64)    // Float
EndfValue::from("text")      // Str
```

#### Accessors

| Method | Returns | Description |
|--------|---------|-------------|
| `as_int()` | `Option<i64>` | Extract as integer (also converts integer-valued floats). |
| `as_float()` | `Option<f64>` | Extract as float (also converts integers). |
| `as_str()` | `Option<&str>` | Extract as string. |
| `as_dict()` | `Option<&IndexMap<...>>` | Extract as dictionary. |
| `as_dict_mut()` | `Option<&mut IndexMap<...>>` | Extract as mutable dictionary. |
| `as_list()` | `Option<&Vec<Option<EndfValue>>>` | Extract as list. |
| `as_list_mut()` | `Option<&mut Vec<...>>` | Extract as mutable list. |
| `as_table()` | `Option<&EndfTable>` | Extract as interpolation table. |
| `is_dict()` | `bool` | Type check. |
| `is_list()` | `bool` | Type check. |

#### Dictionary Operations

```rust
// By key
data.get("AWR")                     // &str keys auto-convert
data.get(EndfKey::Int(3))           // integer keys
data.get_mut("AWR")
data.insert("QM", EndfValue::Float(1.5));
data.contains_key("AWR")

// By path (slash-separated, integers auto-detected)
data.get_path("3/1/QM")            // MF=3 → MT=1 → QM
data.set_path("3/1/QM", value)     // creates intermediate dicts
```

#### Serialization

Two JSON formats are available:

**Human-friendly (recommended):** Clean JSON without type wrappers.

```rust
use endf_parser::json;

let json_str = json::to_json_string(&data, true)?;   // pretty-printed
let restored = json::from_json_string(&json_str)?;
```

Produces:
```json
{
  "3": {
    "1": {
      "ZA": 1001,
      "AWR": 0.9991673,
      "xstable": {
        "NBT": [2],
        "INT": [2],
        "E": [1e-5, 20000000.0],
        "xs": [20.436, 0.548]
      }
    }
  }
}
```

Integer dict keys become string keys (JSON requirement). On deserialization,
numeric-looking keys are restored as `EndfKey::Int`.

**Lossless (serde-derived):** Preserves exact type information.

```rust
let json = serde_json::to_string_pretty(&data)?;
let restored: EndfValue = serde_json::from_str(&json)?;
```

Uses type tags (`"Int"`, `"Float"`, `"Dict"`, etc.) and prefixed keys
(`"i:42"`, `"s:name"`) for lossless roundtrip. More verbose but
distinguishes `Int(0)` from `Float(0.0)`.

---

### `EndfKey`

**Module:** `endf_parser::value`

Dictionary key type — either an integer or a string.

```rust
let k1 = EndfKey::Int(3);           // MF number
let k2 = EndfKey::Str("AWR".into()); // variable name
let k3: EndfKey = 3_i64.into();      // From<i64>
let k4: EndfKey = "AWR".into();      // From<&str>
```

---

### `EndfTable`

**Module:** `endf_parser::value`

Interpolation table from TAB1 or TAB2 records.

| Field | Type | Description |
|-------|------|-------------|
| `nbt` | `Vec<i64>` | Interpolation region boundaries |
| `int` | `Vec<i64>` | Interpolation type codes |
| `x` | `Vec<f64>` | Independent variable (energy). Empty for TAB2. |
| `y` | `Vec<f64>` | Dependent variable (cross section). Empty for TAB2. |

```rust
let table = EndfTable::new_tab1(
    vec![3],           // NBT
    vec![2],           // INT (lin-lin)
    vec![1.0, 2.0, 3.0], // energies
    vec![0.1, 0.2, 0.3], // cross sections
);
assert!(table.is_tab1());
```

---

### `EndfFloat`

**Module:** `endf_parser::endf_float`

A float that optionally preserves its original ENDF field string for
lossless roundtrip. Infrastructure for future `preserve_value_strings`
support.

```rust
let ef = EndfFloat::new(1.23456e7, Some("1.23456+7".to_string()));
assert_eq!(ef.value(), 1.23456e7);
assert_eq!(ef.original_string(), Some("1.23456+7"));
```

---

### `EndfError`

**Module:** `endf_parser::error`

All errors are variants of the `EndfError` enum. Key variants:

| Variant | When |
|---------|------|
| `InvalidFloat` | A number field cannot be parsed as a float. |
| `InvalidInteger` | A number field cannot be parsed as an integer. |
| `NumberMismatch` | A field value does not match the recipe expectation. |
| `VariableNotFound` | A required variable is missing from the data dictionary. |
| `MissingSection` | A required section is missing during write. |
| `UnexpectedEndOfInput` | The file ends before the recipe is satisfied. |
| `RecipeParse` | A recipe file has a syntax error. |
| `Io` | An underlying I/O error (wraps `std::io::Error`). |

The type alias `EndfResult<T>` is `Result<T, EndfError>`.

---

### `RecipeCatalogue`

**Module:** `endf_parser::recipe::catalogue`

Manages the collection of ENDF recipes indexed by (MF, MT). Normally
created internally by `EndfParser`, but available for advanced use.

```rust
use endf_parser::recipe::catalogue::RecipeCatalogue;

// Built-in catalogues
let cat = RecipeCatalogue::endf6()?;
let cat = RecipeCatalogue::endf6_ext()?;
let cat = RecipeCatalogue::for_format("jendl")?;

// Runtime loading
let cat = RecipeCatalogue::load_from_dir(Path::new("my_recipes/"))?;

// Manual construction
let mut cat = RecipeCatalogue::new();
cat.add_recipe_from_str(3, -1, "[MAT,MF,MT/ ZA,AWR,0,LR,NR,NP/ E / xs]TAB1\nSEND\n")?;

// Lookup (exact match, then wildcard MT=-1)
let recipe = cat.get(3, 102);   // looks for (3,102) then (3,-1)
```

Supported built-in formats:

| Format | Description |
|--------|-------------|
| `"endf6"` | Strict ENDF-6 standard |
| `"endf6-ext"` | Extended ENDF-6 (tolerates common deviations) |
| `"jendl"` | JENDL library conventions |
| `"pendf"` | Processed ENDF (NJOY output) |
| `"errorr"` | ERRORR covariance format |

---

## JSON Module

**Module:** `endf_parser::json`

Human-friendly JSON serialization for `EndfValue`. Unlike the serde-derived
format, this produces clean JSON that is easy to read and edit by hand.

| Function | Description |
|----------|-------------|
| `to_json_string(&EndfValue, bool) -> Result<String>` | Serialize to JSON string. Bool controls pretty-printing. |
| `from_json_string(&str) -> Result<EndfValue>` | Deserialize from JSON string. |
| `endf_to_json(&EndfValue) -> serde_json::Value` | Convert to serde_json value (for custom processing). |
| `json_to_endf(&serde_json::Value) -> EndfValue` | Convert from serde_json value. |

**Key mapping rules:**
- `EndfKey::Int(n)` → JSON key `"n"` (string). On read-back, numeric strings become `Int` keys.
- `EndfKey::Str(s)` → JSON key `"s"`. On read-back, non-numeric strings become `Str` keys.
- `EndfValue::Int(n)` → JSON integer. On read-back, exact integers restore as `Int`.
- `EndfValue::Float(f)` → JSON number. On read-back, non-integer numbers restore as `Float`.
- `EndfValue::List` with `None` entries → JSON array with `null`.

---

## Command-Line Interface

The `endf-tool` binary provides ENDF↔JSON conversion from the command line.

### Build

```bash
cargo build --release --bin endf_tool -p endf-parser
```

### ENDF → JSON

```bash
# Human-friendly format (default), pretty-printed
endf-tool endf2json input.endf output.json --pretty

# Compact (no indentation)
endf-tool endf2json input.endf output.json

# Lossless tagged format (preserves Int vs Float distinction)
endf-tool endf2json input.endf output.json --pretty --lossless

# Output to stdout
endf-tool endf2json input.endf - --pretty
```

### JSON → ENDF

```bash
# Accepts both human-friendly and lossless JSON formats
endf-tool json2endf input.json output.endf
```

### Options

| Option | Description |
|--------|-------------|
| `--format <fmt>` | Recipe format: `endf6` (default), `endf6-ext`, `jendl`, `pendf`, `errorr` |
| `--recipes-dir <dir>` | Load custom recipes from directory (overrides `--format`) |
| `--pretty` | Pretty-print JSON output |
| `--lossless` | Use tagged JSON format for exact type preservation |
| `--ignore-mismatches` | Tolerate number/zero/varspec mismatches |
| `--ignore-send` | Ignore SEND/FEND/MEND/TEND record validation |
| `--ignore-blank` | Ignore blank lines in input |
| `--ignore-tpid` | Ignore missing TPID (tape header) record |

### Examples

```bash
# Convert ENDF/B-VIII.1 file to readable JSON
endf-tool endf2json n-026_Fe_056.endf Fe56.json \
    --pretty --ignore-mismatches --ignore-send --ignore-tpid

# Edit Fe56.json in a text editor, then convert back
endf-tool json2endf Fe56.json Fe56_modified.endf \
    --ignore-mismatches --ignore-send --ignore-tpid

# Use extended ENDF-6 recipes (more tolerant parsing)
endf-tool endf2json input.endf output.json --format endf6-ext --pretty

# Use custom recipes
endf-tool endf2json input.endf output.json --recipes-dir ./my_recipes/ --pretty
```

---

## Low-Level Modules

These modules are public and can be used directly for fine-grained control.

### `endf_parser::fortran`

Fortran-style number parsing and formatting. ENDF uses a fixed-width format
where the `E` in scientific notation is often omitted (`1.23456+7` means
`1.23456e7`).

```rust
use endf_parser::fortran::*;
use endf_parser::options::{ReadOpts, WriteOpts};

let v = fortstr_to_f64("1.23456+7", &ReadOpts::default())?;  // 1.23456e7
let s = f64_to_fortstr(1.23456e7, &WriteOpts::default());     // " 1.23456+7"
let n = read_fort_int("  42")?;                                 // 42
```

### `endf_parser::records`

Low-level ENDF record I/O. Each ENDF line is 80 characters: 6 data fields
of 11 characters + 9 characters of control info (MAT/MF/MT) + optional 5-digit
line number.

```rust
use endf_parser::records::*;
use endf_parser::options::ReadOpts;

let (cont, ctrl) = read_cont(line, &ReadOpts::default())?;
println!("C1={}, L1={}, MAT={}", cont.c1, cont.l1, ctrl.mat);
```

### `endf_parser::sections`

Split an ENDF file into MF/MT sections and manage line numbers.

```rust
use endf_parser::sections::split_sections;
use endf_parser::options::ReadOpts;

let lines: Vec<&str> = content.lines().collect();
let sections = split_sections(&lines, &ReadOpts::default())?;
// sections: BTreeMap<i32, BTreeMap<i32, Vec<String>>>
for (mf, mt_map) in &sections {
    for (mt, lines) in mt_map {
        println!("MF{}/MT{}: {} lines", mf, mt, lines.len());
    }
}
```

---

## Examples

### Parse and inspect

```rust
let parser = EndfParser::new()?;
let data = parser.parse_file(Path::new("Cu-63.endf"))?;

// Iterate over all MF/MT sections
let top = data.as_dict().unwrap();
for (mf_key, mf_val) in top {
    let mt_dict = mf_val.as_dict().unwrap();
    for (mt_key, section) in mt_dict {
        match section {
            EndfValue::Dict(d) => println!("MF{}/MT{}: {} variables", mf_key, mt_key, d.len()),
            EndfValue::Str(_)  => println!("MF{}/MT{}: unparsed", mf_key, mt_key),
            _ => {}
        }
    }
}
```

### Modify cross section data

```rust
let parser = EndfParser::new()?;
let mut data = parser.parse_file(Path::new("input.endf"))?;

// Scale all cross sections in MF3/MT102 by a factor of 1.1
let xs_path = "3/102/xstable/xs";
if let Some(EndfValue::List(xs)) = data.get_path(xs_path).cloned() {
    let scaled: Vec<Option<EndfValue>> = xs.iter().map(|v| {
        v.as_ref().map(|val| EndfValue::Float(val.as_float().unwrap() * 1.1))
    }).collect();
    data.set_path(xs_path, EndfValue::List(scaled));
}

parser.write_file(Path::new("output.endf"), &data)?;
```

### Roundtrip with JSON intermediate

```rust
use endf_parser::json;

let parser = EndfParser::new()?;
let data = parser.parse_file(Path::new("input.endf"))?;

// Save as human-friendly JSON
let json_str = json::to_json_string(&data, true)?;  // true = pretty-print
std::fs::write("data.json", &json_str)?;

// Load from JSON and write back to ENDF
let restored = json::from_json_string(&std::fs::read_to_string("data.json")?)?;
parser.write_file(Path::new("output.endf"), &restored)?;
```

### Use a different recipe format

```rust
let parser = EndfParser::builder()
    .endf_format("pendf")
    .build()?;
let data = parser.parse_file(Path::new("pendf_output.endf"))?;
```

### Load custom recipes at runtime

```rust
let parser = EndfParser::builder()
    .recipes_dir("/path/to/my/recipes/")
    .build()?;
```

---

## Compiled Parser (`endf-parser-compiled`)

The `endf-parser-compiled` crate provides statically generated parse and write
functions for all ENDF-6 recipes. Instead of interpreting the recipe AST at
runtime, the recipe-to-Rust compiler translates each recipe into a dedicated
Rust function at build time, eliminating interpreter dispatch overhead.

### When to Use

| Use case | Recommended |
|----------|-------------|
| Parse many files quickly | **Compiled** — bypasses interpreter overhead |
| Optimization loops (parse → modify → write → repeat) | **Compiled** — fast read and write |
| Custom recipes or runtime recipe loading | **Interpreter** — compiled only supports built-in recipes |
| Write-only (no parsing) | Either — both produce identical output |

### Reading with the Compiled Parser

```rust
use endf_parser::sections::split_sections;
use endf_parser::options::{ReadOpts, ParseOpts};
use endf_parser::value::{EndfKey, EndfValue};

let content = std::fs::read_to_string("neutrons.endf")?;
let lines: Vec<&str> = content.lines().collect();

let read_opts = ReadOpts {
    ignore_send_records: true,
    ignore_missing_tpid: true,
    ..ReadOpts::default()
};
let parse_opts = ParseOpts {
    ignore_number_mismatch: true,
    ignore_zero_mismatch: true,
    ..ParseOpts::default()
};

// Split file into MF/MT sections
let section_map = split_sections(&lines, &read_opts)?;

// Parse each section with the compiled parser
for (mf, mt_map) in &section_map {
    for (mt, section_lines) in mt_map {
        let data = endf_parser_compiled::parse_section(
            *mf, *mt, section_lines, &read_opts, &parse_opts
        )?;
        // data is an EndfValue::Dict, same structure as the interpreter
    }
}
```

### Writing with the Compiled Parser

```rust
use endf_parser::options::WriteOpts;
use endf_parser::value::EndfValue;

let write_opts = WriteOpts::default();

// data is an EndfValue::Dict from a prior parse
let lines: Vec<String> = endf_parser_compiled::write_section(
    3, 1, &data, &write_opts
)?;

// lines contains the ENDF-formatted text (one string per line, without SEND)
```

The `write_section` function takes:
- `mf: i32` — the MF number
- `mt: i32` — the MT number
- `data: &EndfValue` — the section dictionary (as returned by `parse_section`)
- `write_opts: &WriteOpts` — formatting options

It returns `Vec<String>` — one entry per ENDF line, without the SEND record.
The caller is responsible for appending SEND/FEND/MEND/TEND records when
assembling a complete file.

### Roundtrip Example

```rust
use endf_parser::sections::split_sections;
use endf_parser::records;
use endf_parser::options::{ReadOpts, ParseOpts, WriteOpts};
use endf_parser::value::{EndfKey, EndfValue};

let read_opts = ReadOpts {
    ignore_send_records: true,
    ignore_missing_tpid: true,
    ..ReadOpts::default()
};
let parse_opts = ParseOpts {
    ignore_number_mismatch: true,
    ignore_zero_mismatch: true,
    ..ParseOpts::default()
};
let write_opts = WriteOpts::default();

// Parse
let content = std::fs::read_to_string("input.endf")?;
let lines: Vec<&str> = content.lines().collect();
let sections = split_sections(&lines, &read_opts)?;

let mut output_lines: Vec<String> = Vec::new();
let mut mat = 0i32;

for (mf, mt_map) in &sections {
    for (mt, section_lines) in mt_map {
        // Read with compiled parser
        let data = endf_parser_compiled::parse_section(
            *mf, *mt, section_lines, &read_opts, &parse_opts
        )?;

        // Extract MAT for control records
        if let Some(m) = data.get("MAT").and_then(|v| v.as_int()) {
            mat = m as i32;
        }

        // Modify data here if needed...

        // Write back with compiled parser
        let written = endf_parser_compiled::write_section(
            *mf, *mt, &data, &write_opts
        )?;
        output_lines.extend(written);
        output_lines.push(records::write_send(mat, *mf, &write_opts));
    }
    output_lines.push(records::write_fend(mat, &write_opts));
}
output_lines.push(records::write_mend(&write_opts));
output_lines.push(records::write_tend(&write_opts));

std::fs::write("output.endf", output_lines.join("\n"))?;
```

### Regenerating the Compiled Parser

The compiled parser source is generated from the recipe catalogue:

```bash
cd rust-test
cargo run --bin compile_recipes -p endf-parser > crates/endf-parser-compiled/src/lib.rs
cargo build --release -p endf-parser-compiled
```

This generates both `parse_mfX_mtY` and `write_mfX_mtY` functions for every
recipe in the ENDF-6 catalogue, plus dispatch functions `parse_section` and
`write_section`.

### Interoperability

The compiled parser produces the same `EndfValue` data structure as the
interpreter. Data parsed by the compiled parser can be written by the
interpreter and vice versa. This has been validated on the entire ENDF/B-VIII.1
library (96,367 sections, 558 files):

- **Compiled read → Compiled write → Compiled re-read**: 95,809/95,809 match
- **Compiled read → Compiled write → Interpreter read**: 96,367/96,367 match
- **Interpreter read vs Compiled read**: 96,367/96,367 match

### Write Mode Details

In write mode, the compiled functions:

1. Read scalar variables from the `EndfValue::Dict` using `get_float`/`get_int`
2. Navigate into named sections (`xstable`, `subsection[k]`, etc.) via `.get()`
3. Construct ENDF records (`ContRecord`, `Tab1Body`, etc.) from the data
4. Format lines using `write_cont`, `write_tab1_body`, etc.

**Scope handling:** Variables from parent sections are accessible via a
fallback chain. The write functions maintain a `_saved_data` reference to
the parent scope so that conditions like `MT==MT1` (where `MT` comes from
the top-level control record) work correctly inside nested sections.

**Conditionals:** Unlike read mode, write mode does not need lookahead
speculation. All variables are already present in the data dictionary,
so conditions are evaluated directly.

**Abbreviations:** Recipe abbreviations (e.g., `NT := NE*(NE-1)+1`) are
expanded at compile time. In write mode, the expanded expression is
evaluated from the data dictionary to compute field values like N1.

**Supported record types:** HEAD/CONT, TAB1, TAB2, LIST (with nested
body loops), TEXT, DIR, INTG, for-loops, if/elif/else, sections
(simple and indexed), abbreviations.
