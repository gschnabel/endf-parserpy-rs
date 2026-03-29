# endf

A recipe-driven toolkit for reading, writing, and modifying
[ENDF-6](https://www.nndc.bnl.gov/endfdocs/ENDF-102-2023.pdf) nuclear data
files in Rust.

Based on the formal ENDF grammar from
[endf-parserpy](https://github.com/IAEA-NDS/endf-parserpy), this crate
uses a set of ENDF recipes (format descriptions) to parse any MF/MT section
into structured data and write it back to valid ENDF-6 format.

## Quick Start

```rust
use endf::parser::EndfParser;
use std::path::Path;

let parser = EndfParser::new()?;
let data = parser.parse_file(Path::new("neutrons.endf"))?;

// Access MF=3, MT=1, variable "QM"
if let Some(qm) = data.get_path("3/1/QM") {
    println!("QM = {}", qm);
}

// Modify and write back
let mut data = data;
data.set_path("3/1/QM", endf::value::EndfValue::Float(1.5));
let output = parser.write(&data)?;
std::fs::write("modified.endf", output)?;
```

## Features

- **Recipe-driven** -- parses and writes ENDF-6 files using a formal grammar,
  supporting five recipe formats: endf6, endf6-ext, jendl, pendf, errorr
- **Roundtrip support** -- parse, modify values, and write back to valid
  ENDF-6 format
- **JSON conversion** -- convert between ENDF and JSON representations
- **Parallel parsing** -- optional multi-threaded section parsing via rayon
  (enabled by default, disable with `default-features = false` for WASM)
- **No unsafe code**

## Recipe Formats

| Format | Description |
|--------|-------------|
| `endf6` | Standard ENDF-6 (default) |
| `endf6-ext` | Extended ENDF-6 with obsolete LVT support |
| `jendl` | JENDL-specific MF8/MT457 handling |
| `pendf` | Pointwise ENDF (NJOY output) |
| `errorr` | ERRORR covariance format (NJOY output) |

```rust
let parser = EndfParser::builder()
    .endf_format("endf6-ext")
    .ignore_send_records(true)
    .build()?;
```

## License

[MIT](https://github.com/gschnabel/endf-parserpy-rs/blob/main/LICENSE)
