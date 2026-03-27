# endf-parser-py — Python API Documentation

Python bindings for the Rust-based ENDF-6 toolkit. Provides high-performance
reading, writing, and modification of ENDF-6 nuclear data files via PyO3.

## Installation

```bash
# Build with maturin (development)
cd rust-test
maturin develop --release

# Or build a wheel
maturin build --release
pip install target/wheels/endf_parser_py-*.whl
```

## Quick Start

```python
from endf_parser_py import EndfParser

# Parse an ENDF file
parser = EndfParser()
data = parser.parsefile("neutrons.endf")

# Access data: MF=3, MT=1
section = data[3][1]
print(f"ZA = {section['ZA']}")
print(f"AWR = {section['AWR']}")

# Access cross section table
xstable = section["xstable"]
energies = xstable["E"]
cross_sections = xstable["xs"]

# Modify and write back
section["QM"] = 1.5
output = parser.write(data)
with open("modified.endf", "w") as f:
    f.write(output)
```

---

## `EndfParser`

The main class. Wraps the Rust `EndfParser` with automatic conversion between
Rust `EndfValue` and Python native types.

### Constructor

```python
EndfParser(**kwargs)
```

All arguments are optional keyword arguments with sensible defaults. The
defaults match the Python `endf-parserpy` package conventions.

#### Parse Options

| Keyword | Type | Default | Description |
|---------|------|---------|-------------|
| `ignore_number_mismatch` | `bool` | `False` | Tolerate mismatches for fields marked with `?` (DesiredNumber) in recipes. |
| `ignore_zero_mismatch` | `bool` | `True` | Tolerate non-zero values where the recipe specifies `0`. |
| `ignore_varspec_mismatch` | `bool` | `True` | Tolerate mismatches for variables marked with `?` (inconsistent var spec). |
| `fuzzy_matching` | `bool` | `False` | Use floating-point tolerance for validation instead of exact equality. |
| `array_type` | `str` | `"dict"` | `"dict"` for sparse index dicts, `"list"` for dense Python lists. |

#### Read Options

| Keyword | Type | Default | Description |
|---------|------|---------|-------------|
| `accept_spaces` | `bool` | `True` | Strip spaces inside number fields. |
| `preserve_value_strings` | `bool` | `False` | Reserved for future lossless roundtrip support. |
| `ignore_blank_lines` | `bool` | `False` | Skip blank lines in the input. |
| `ignore_send_records` | `bool` | `False` | Don't validate section-end records. |
| `ignore_missing_tpid` | `bool` | `False` | Allow files without a tape-header record. |
| `width` | `int` | `11` | Field width in characters. |
| `nofail` | `bool` | `False` | On section parse failure, store raw lines instead of raising. |

#### Write Options

| Keyword | Type | Default | Description |
|---------|------|---------|-------------|
| `abuse_signpos` | `bool` | `True` | Use sign position for extra precision on positive numbers. |
| `skip_intzero` | `bool` | `True` | Omit leading zero: `0.12` → `.12`. |
| `prefer_noexp` | `bool` | `True` | Prefer fixed-point when equally precise. |
| `keep_E` | `bool` | `False` | Keep `E` in scientific notation (`1.23E+7` vs `1.23+7`). |
| `include_linenum` | `bool` | `True` | Add 5-digit line numbers to output. |
| `strict_datatypes` | `bool` | `False` | Error on float-to-int conversion loss. |
| `zero_as_blank` | `bool` | `False` | Write zeros as blanks in end records. |

#### Recipe Options

| Keyword | Type | Default | Description |
|---------|------|---------|-------------|
| `endf_format` | `str` | `"endf6"` | Recipe flavour: `"endf6"`, `"endf6-ext"`, `"jendl"`, `"pendf"`, `"errorr"`. |
| `recipes_dir` | `str` | *none* | Path to a directory of `.recipe` files (overrides `endf_format`). |

#### Examples

```python
# Default parser
parser = EndfParser()

# Tolerant parser for messy files
parser = EndfParser(
    ignore_number_mismatch=True,
    ignore_blank_lines=True,
    ignore_send_records=True,
    ignore_missing_tpid=True,
    nofail=True,
)

# PENDF format
parser = EndfParser(endf_format="pendf")

# Custom recipes from disk
parser = EndfParser(recipes_dir="/path/to/recipes/")
```

---

### Methods

#### `parse(input: str) -> dict`

Parse ENDF-6 formatted text into a nested dictionary.

**Parameters:**
- `input` (`str`): The full ENDF file content as a string.

**Returns:** A nested dictionary `{mf: {mt: section_data}}`.

**Raises:** `RuntimeError` on parse failure (unless `nofail=True`).

```python
with open("neutrons.endf") as f:
    content = f.read()
data = parser.parse(content)
```

---

#### `parsefile(filename: str) -> dict`

Parse an ENDF-6 file from disk.

**Parameters:**
- `filename` (`str`): Path to the ENDF file.

**Returns:** A nested dictionary `{mf: {mt: section_data}}`.

**Raises:** `RuntimeError` on parse or I/O failure.

```python
data = parser.parsefile("neutrons.endf")
```

---

#### `write(endf_dict: dict) -> str`

Write a nested dictionary back to ENDF-6 formatted text.

**Parameters:**
- `endf_dict` (`dict`): Nested dictionary as returned by `parse()`/`parsefile()`.

**Returns:** The ENDF-6 formatted text as a single string.

**Raises:** `RuntimeError` on write failure.

```python
output = parser.write(data)
with open("output.endf", "w") as f:
    f.write(output)
```

---

#### `writefile(filename: str, endf_dict: dict) -> None`

Write a nested dictionary directly to a file.

**Parameters:**
- `filename` (`str`): Output file path.
- `endf_dict` (`dict`): Nested dictionary as returned by `parse()`/`parsefile()`.

**Raises:** `RuntimeError` on write or I/O failure.

```python
parser.writefile("output.endf", data)
```

---

## Data Structure

The parsed data is a nested Python dictionary mirroring the ENDF file
structure:

```python
{
    mf_number: {                    # int → MF file number
        mt_number: {                # int → MT reaction type
            "MAT": 2925,            # int: material number
            "MF": 3,                # int: file number
            "MT": 1,                # int: reaction type
            "ZA": 29063.0,          # float: Z*1000 + A
            "AWR": 62.389,          # float: mass ratio
            "QM": 0.0,             # float: Q-value (mass difference)
            "LR": 0,               # int: flag
            "xstable": {            # dict: named subsection (TAB1 table)
                "NBT": [3],         # list: interpolation boundaries
                "INT": [2],         # list: interpolation codes
                "E": [1e-5, ...],   # list: energies
                "xs": [0.1, ...],   # list: cross sections
            },
            "subsection_name": {    # dict: recipe-defined section
                ...
            },
        },
        ...
    },
    ...
}
```

### Type Mapping

| ENDF Concept | Python Type | Notes |
|-------------|-------------|-------|
| Integer field | `int` | MAT, MF, MT, counters, flags |
| Float field | `float` | Physical quantities |
| Text field | `str` | TEXT record content |
| Section | `dict` | Named recipe sections |
| Array (dict mode) | `dict` | `{1: value, 2: value, ...}` |
| Array (list mode) | `list` | `[value, value, ...]` with `None` gaps |
| TAB1/TAB2 table | `dict` | `{"NBT": [...], "INT": [...], "X": [...], "Y": [...]}` |
| Unparsed section | `str` | Raw ENDF lines (when `nofail=True` and parse failed) |

### Accessing Data

```python
data = parser.parsefile("input.endf")

# By MF/MT numbers (integer keys)
mf3 = data[3]              # all MF=3 sections
mt1 = data[3][1]           # MF=3, MT=1 section
za = data[3][1]["ZA"]      # specific variable

# Iterate over sections
for mf, mt_dict in data.items():
    for mt, section in mt_dict.items():
        if isinstance(section, dict):
            print(f"MF{mf}/MT{mt}: {len(section)} variables")
        else:
            print(f"MF{mf}/MT{mt}: unparsed")

# Access table data
table = data[3][1]["xstable"]
energies = table["E"]           # list of floats
xs = table["xs"]                # list of floats
nbt = table["NBT"]             # list of ints (interpolation boundaries)
interp = table["INT"]           # list of ints (interpolation codes)
```

### Modifying Data

```python
# Modify scalar values
data[3][1]["QM"] = 1.5

# Scale cross sections
xs = data[3][1]["xstable"]["xs"]
data[3][1]["xstable"]["xs"] = [v * 1.1 for v in xs]

# Add a new variable
data[3][1]["custom_var"] = 42

# Write modified data
parser.writefile("output.endf", data)
```

---

## Examples

### Parse, inspect, and write

```python
from endf_parser_py import EndfParser

parser = EndfParser()
data = parser.parsefile("n-029_Cu_063.endf")

# Print all sections
for mf in sorted(data.keys()):
    for mt in sorted(data[mf].keys()):
        sec = data[mf][mt]
        if isinstance(sec, dict):
            mat = sec.get("MAT", "?")
            print(f"  MF={mf} MT={mt:>4}  MAT={mat}  ({len(sec)} fields)")

# Roundtrip: parse → write → re-parse
output = parser.write(data)
data2 = parser.parse(output)
```

### Extract cross section data for plotting

```python
import matplotlib.pyplot as plt
from endf_parser_py import EndfParser

parser = EndfParser()
data = parser.parsefile("n-029_Cu_063.endf")

# MF3/MT1 = total cross section
table = data[3][1]["xstable"]
E = table["E"]
xs = table["xs"]

plt.loglog(E, xs)
plt.xlabel("Energy (eV)")
plt.ylabel("Cross Section (barns)")
plt.title("Cu-63 Total Cross Section")
plt.show()
```

### Batch processing

```python
import os
from endf_parser_py import EndfParser

parser = EndfParser(
    ignore_number_mismatch=True,
    ignore_blank_lines=True,
    ignore_send_records=True,
    ignore_missing_tpid=True,
    nofail=True,
)

endf_dir = "/path/to/endf/library/"
for filename in sorted(os.listdir(endf_dir)):
    if not filename.endswith(".endf"):
        continue
    path = os.path.join(endf_dir, filename)
    data = parser.parsefile(path)

    # Count parsed vs unparsed sections
    parsed = sum(
        1 for mf in data.values()
        for sec in mf.values()
        if isinstance(sec, dict)
    )
    total = sum(len(mf) for mf in data.values())
    print(f"{filename}: {parsed}/{total} sections parsed")
```

### Compare two ENDF files

```python
from endf_parser_py import EndfParser

parser = EndfParser()
data_a = parser.parsefile("file_a.endf")
data_b = parser.parsefile("file_b.endf")

for mf in sorted(set(data_a.keys()) | set(data_b.keys())):
    mts_a = set(data_a.get(mf, {}).keys())
    mts_b = set(data_b.get(mf, {}).keys())

    for mt in sorted(mts_a - mts_b):
        print(f"MF{mf}/MT{mt}: only in file A")
    for mt in sorted(mts_b - mts_a):
        print(f"MF{mf}/MT{mt}: only in file B")
    for mt in sorted(mts_a & mts_b):
        # Compare specific values
        sa = data_a[mf][mt]
        sb = data_b[mf][mt]
        if isinstance(sa, dict) and isinstance(sb, dict):
            for key in set(sa.keys()) | set(sb.keys()):
                if sa.get(key) != sb.get(key):
                    print(f"MF{mf}/MT{mt}/{key}: differs")
```

### Use custom recipes

```python
from endf_parser_py import EndfParser

# Load recipes from a directory
parser = EndfParser(recipes_dir="/path/to/custom/recipes/")
data = parser.parsefile("custom_format.endf")
```

---

## Comparison with endf-parserpy

This package provides the same core functionality as the Python
[endf-parserpy](https://github.com/IAEA-NDS/endf-parserpy) package but
implemented in Rust for performance. Key differences:

| Feature | endf-parserpy | endf-parser-py (this package) |
|---------|--------------|-------------------------------|
| Language | Python (with optional C++ compiled parsers) | Rust (via PyO3) |
| Recipe system | Lark grammar + Python interpreter | Pest grammar + Rust interpreter |
| Data structure | Nested Python dicts | Nested Python dicts (identical) |
| Parse speed | ~1x (Python) / ~50-100x (C++) | Comparable to C++ compiled parsers |
| Write speed | ~1x (Python) / ~50-100x (C++) | Comparable to C++ compiled parsers |
| `EndfDict` / `EndfPath` | Yes | Not yet (use plain dict access) |
| `EndfFloat` | Yes | Infrastructure in place, not yet exposed |
| `explain()` | Yes | Not yet |
| Recipe formats | endf6, endf6-ext, jendl, pendf, errorr | Same five formats |
| Runtime recipe loading | Cache directory | `recipes_dir` parameter |

### Migration from endf-parserpy

```python
# endf-parserpy
from endf_parserpy import EndfParserPy
parser = EndfParserPy(ignore_zero_mismatch=True)
data = parser.parsefile("input.endf")

# endf-parser-py (this package) — same API shape
from endf_parser_py import EndfParser
parser = EndfParser(ignore_zero_mismatch=True)
data = parser.parsefile("input.endf")

# Data access is identical
za = data[3][1]["ZA"]
```
