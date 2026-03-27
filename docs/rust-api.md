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

`EndfValue` implements `serde::Serialize` and `serde::Deserialize`, enabling
JSON roundtrip:

```rust
let json = serde_json::to_string_pretty(&data)?;
let restored: EndfValue = serde_json::from_str(&json)?;
```

Note: `EndfKey` serializes integers as `"i:42"` and strings as `"s:name"`
to preserve type information in JSON object keys.

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
let parser = EndfParser::new()?;
let data = parser.parse_file(Path::new("input.endf"))?;

// Save as JSON
let json = serde_json::to_string_pretty(&data)?;
std::fs::write("data.json", &json)?;

// Load from JSON and write back to ENDF
let restored: EndfValue = serde_json::from_str(&json)?;
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
