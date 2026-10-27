---
title: pyforge-jupyter
description: "PyForge: Jupyter notebooks - nbformat v4 parsing and round-trip stable serialization, rendered outputs with sanitized HTML and SVG, DataFrame tables, cell-by-cell notebook diffs and the Jupyter messaging protocol; kernels over ZeroMQ and Jupyter servers with the runtime feature."
order: 297
tags: [crate, rust, pack, python, jupyter, notebook, nbformat, zeromq, kernel, dataframe, diff]
---

# pyforge-jupyter

`packs/pyforge/jupyter` — PyForge's Jupyter support (plugin `forge.jupyter`, see
[Jupyter notebooks](../../user/jupyter.md)). The default build is pure and also compiled to
WebAssembly (`crates/forge-wasm/src/jupyter.rs`); the desktop app (`commands/jupyter.rs`)
adds the features `runtime` (kernels) and `keychain` (server tokens).

## Purpose

- `nbformat`: `.ipynb` (nbformat 4.x) into a `Notebook` (cells with joined texts, typed
  outputs, metadata, the cell's `origin` in the file) and back. `serialize` merges the editor's
  model into the original JSON: unchanged cells and outputs keep their raw JSON, changed
  fields keep the original's form (string or list of lines), new cells and outputs are written
  like Jupyter writes them (sorted keys, lines). The indent, compact one-line files, `\u`
  escapes of non-ASCII text and the final line break are detected and kept; key order is read into the
  crate's own ordered JSON values (`ordered`), so serde_json's `preserve_order` stays off for
  the workspace. nbformat 3 and older are refused.
- `output`: stream, `execute_result`, `display_data` and error outputs with MIME bundles
  (texts joined, JSON types as JSON) and `render`, the representation the editor shows:
  console lines from `forge-output` (ANSI, `\r`, file links), sanitized HTML, data URLs for
  images (SVG sanitized), JSON, Markdown or a DataFrame, chosen by JupyterLab's MIME order.
- `html`: a small tokenizer with `sanitize_html` (allowlist of elements and attributes,
  http(s)/mailto links, `data:image` sources, CSS without `url()`), `sanitize_svg` (scripts,
  event handlers, `foreignObject`, doctypes and external references removed) and `tables`.
- `dataframe`: pandas' HTML (index names, MultiIndex columns and rows through colspan /
  rowspan, the "N rows × M columns" footer), Table Schema JSON
  (`application/vnd.dataresource+json`) and the helper's View as DataFrame JSON into one
  `DataFrame`; `query` sorts (numeric-aware, `forge_viewers::sort_order`), filters and pages.
- `diff`: notebook diffs. Cells are paired by id, or by a patience alignment of their sources
  with neighbouring removed and added cells as changed cells; sources and outputs (as text,
  images as size and hash) get separate line diffs (`similar`).
- `protocol`: messages, HMAC-SHA256 signing and constant-time checks, the ZeroMQ frames
  (`<IDS|MSG>`), the WebSocket JSON form, connection files, `KernelEvent`s of iopub and shell
  messages and `user_expressions` replies.
- `runtime`: `kernel` (DEALER sockets for shell and control, SUB for iopub with the pure-Rust
  `zeromq` crate, MIT; five free ports from [`forge-ports`](forge-ports.md) and private
  connection files), `server` (REST `/api/kernels`, `/api/kernelspecs` and the kernel
  WebSocket with `tokio-tungstenite`) and
  `session` (runs cells and reports `SessionEvent`s per cell id with rendered outputs; waits
  for both the reply and the idle status; Variables, children and View as DataFrame through
  the helper `forge_jupyter.py` and `user_expressions`; interrupt with `interrupt_request`).
- `keychain`: Jupyter server tokens in the OS keychain (service `forge-jupyter`).

## Tests

Unit tests per module, fixture notebooks in `tests/fixtures` (written by Python's `json.dumps`
exactly as Jupyter writes them, plus an nbformat 4.2 file with another style) that round-trip
byte for byte, and `tests/fake_kernel.rs`: the session against an in-process fake kernel on
127.0.0.1 that checks every signature (a wrong key gets no answers). No network:
`cargo test -p pyforge-jupyter --features runtime`. `tests/ipykernel_smoke.rs` runs a real
ipykernel (ignored; `FORGE_JUPYTER_PYTHON=/path/to/python … -- --ignored`).
