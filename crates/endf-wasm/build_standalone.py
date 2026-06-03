#!/usr/bin/env python3
"""Assemble a fully self-contained, offline ENDF Explorer HTML file.

Inlines the wasm-bindgen `no-modules` JS glue and base64-embeds the .wasm
binary so the page runs from a file:// URL with no HTTP server and no network.

Run from crates/endf-wasm after:
    wasm-pack build --target no-modules --release --out-dir pkg-nomod
"""
import base64
import pathlib
import sys

HERE = pathlib.Path(__file__).parent
SRC_HTML = HERE / "www" / "index.html"
GLUE_JS = HERE / "pkg-nomod" / "endf_wasm.js"
WASM = HERE / "pkg-nomod" / "endf_wasm_bg.wasm"
OUT = HERE / "www" / "endf-explorer-offline.html"

for p in (SRC_HTML, GLUE_JS, WASM):
    if not p.exists():
        sys.exit(f"missing required input: {p}")

html = SRC_HTML.read_text()
glue = GLUE_JS.read_text()
wasm_b64 = base64.b64encode(WASM.read_bytes()).decode("ascii")

# Split off the original ES-module <script> (everything from it to </html>)
marker = '<script type="module">'
idx = html.index(marker)
head_markup = html[:idx]

# Emphasize the offline story in the header subtitle.
head_markup = head_markup.replace(
    "client-side parsing via WebAssembly",
    "100% offline &middot; client-side WebAssembly &middot; no server, no internet",
)

# Extract the original app script body (between the module tag and </script>).
rest = html[idx + len(marker):]
app_js = rest[: rest.index("</script>")]

# Adapt the app code: drop the ES import, point init + class at the global
# `wasm_bindgen` produced by the inlined no-modules glue, and feed it the
# embedded bytes instead of fetching a .wasm file.
app_js = app_js.replace(
    "import init, { WasmEndfParser } from '../pkg/endf_wasm.js';\n", ""
)
app_js = app_js.replace("await init();", "await wasm_bindgen(__ENDF_WASM_BYTES);")
app_js = app_js.replace("new WasmEndfParser(", "new wasm_bindgen.WasmEndfParser(")

loader = (
    "// --- embedded wasm (base64) -> bytes; no fetch, no network ---\n"
    f'const __ENDF_WASM_B64 = "{wasm_b64}";\n'
    "const __ENDF_WASM_BYTES = (() => {\n"
    "  const bin = atob(__ENDF_WASM_B64);\n"
    "  const bytes = new Uint8Array(bin.length);\n"
    "  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);\n"
    "  return bytes;\n"
    "})();\n"
)

out = (
    head_markup
    + "<!-- wasm-bindgen no-modules glue (inlined) -->\n"
    + "<script>\n" + glue + "\n</script>\n"
    + "<!-- embedded wasm + application -->\n"
    + "<script>\n" + loader + app_js + "</script>\n"
    + "</body>\n</html>\n"
)

OUT.write_text(out)
size_mb = OUT.stat().st_size / 1024 / 1024
print(f"wrote {OUT} ({size_mb:.2f} MB, single self-contained file)")
