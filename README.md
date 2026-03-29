# endf-parserpy-rs

A high-performance Rust toolkit for reading, writing, and modifying
[ENDF-6](https://www.nndc.bnl.gov/endfdocs/ENDF-102-2023.pdf) nuclear data
files. Based on the recipe-driven approach of
[endf-parserpy](https://github.com/IAEA-NDS/endf-parserpy), reimplemented
in Rust for speed and safety.

## Features

- **Recipe-driven parsing** -- uses a formal grammar of ENDF recipes to parse
  and write any MF/MT section, supporting the same recipe formats as
  endf-parserpy (endf6, endf6-ext, jendl, pendf, errorr)
- **Two parsing backends** -- a flexible recipe interpreter and a compiled
  parser with statically generated Rust code for each MF/MT combination
- **Python bindings** -- drop-in replacement for endf-parserpy with identical
  data structures (nested dicts), via PyO3
- **CLI tool** -- `endf-tool` for converting between ENDF and JSON formats
- **Lossless roundtrip** -- parse an ENDF file, modify values, and write it
  back preserving formatting conventions

## Project Structure

| Crate | Description |
|-------|-------------|
| `endf-parser` | Core library: recipe grammar, interpreter, records, Fortran number formatting, JSON conversion |
| `endf-parser-compiled` | Compiled parser with generated Rust functions for each MF/MT |
| `endf-parserpy-rs` | Python bindings via PyO3 (exposes `EndfParser` and `CompiledParser`) |

## Installation

### Python (via maturin)

Requires Rust >= 1.83 and [maturin](https://www.maturin.rs/).

```bash
pip install maturin

cd crates/endf-parserpy-rs
maturin develop --release
```

### Rust

Add to your `Cargo.toml`:

```toml
[dependencies]
endf-parser = { path = "crates/endf-parser" }
```

### CLI tool

```bash
cargo build --release --bin endf_tool
```

## Performance

Parsing the complete ENDF/B-VIII.1 neutron sublibrary (558 files, 1.28 GB)
using the `endf6-ext` recipe format, called from Python:

| Parser | Time | Speed |
|--------|-----:|------:|
| endf-parserpy interpreter (Python) | ~3971s | 0.32 MB/s |
| endf-parserpy C++ compiled parser | 28s | 46.6 MB/s |
| endf-parserpy-rs interpreter | 73s | 17.5 MB/s |
| endf-parserpy-rs compiled parser | 40s | 32.0 MB/s |

The Python interpreter time is extrapolated from a stratified sample
(35% of the library). All other parsers were benchmarked on the full
library. Times include Python-side overhead (dict conversion).

Pure Rust performance (no Python overhead):

| Parser | Time | Speed |
|--------|-----:|------:|
| Rust interpreter (endf6-ext) | 58s | 22.0 MB/s |
| Rust compiled parser (endf6) | 32s | 40.0 MB/s |

## Quick Start

### Python

```python
from endf_parserpy_rs import EndfParser

parser = EndfParser()
data = parser.parsefile("neutrons.endf")

# Access MF=3, MT=1
section = data[3][1]
print(f"ZA = {section['ZA']}, AWR = {section['AWR']}")

# Modify and write back
section["QM"] = 1.5
parser.writefile("modified.endf", data)
```

The compiled parser offers better performance for the built-in recipes:

```python
from endf_parserpy_rs import CompiledParser

parser = CompiledParser()
data = parser.parsefile("neutrons.endf")
```

### Rust

```rust
use endf_parser::parser::EndfParser;
use std::path::Path;

let parser = EndfParser::new()?;
let data = parser.parse_file(Path::new("neutrons.endf"))?;

if let Some(qm) = data.get_path("3/1/QM") {
    println!("QM = {}", qm);
}

let output = parser.write(&data)?;
std::fs::write("modified.endf", output)?;
```

### CLI

```bash
# ENDF to JSON
endf_tool endf2json input.endf output.json --pretty

# JSON to ENDF
endf_tool json2endf output.json roundtrip.endf

# Parallel parsing
endf_tool endf2json input.endf output.json --threads 0
```

## Relationship to endf-parserpy

This project reimplements the core of
[endf-parserpy](https://github.com/IAEA-NDS/endf-parserpy) in Rust.
The Python bindings provide the same API shape and identical data structures
(nested Python dicts keyed by MF/MT), making migration straightforward:

```python
# endf-parserpy
from endf_parserpy import EndfParserPy
parser = EndfParserPy(ignore_zero_mismatch=True)

# endf-parserpy-rs (this project)
from endf_parserpy_rs import EndfParser
parser = EndfParser(ignore_zero_mismatch=True)
```

## References

The recipe-driven approach used in this project is described in:

> G. Schnabel, D. Lopez Aldama, R. Capote,
> *How to explain ENDF-6 to computers: A formal ENDF format description language*,
> [arXiv:2312.08249](https://arxiv.org/abs/2312.08249) (2023)

## Documentation

- [Python API](docs/python-api.md)
- [Rust API](docs/rust-api.md)

## License

[MIT](LICENSE)

Copyright (c) 2022-2025 International Atomic Energy Agency
Copyright (c) 2025-2026 Georg Schnabel
