---
title: forge-viewers
description: "Forge file viewers: image metadata for the image editor and CSV/TSV tables for Edit as Table (pure, also built to WebAssembly)."
order: 268
tags: [crate, rust]
---

# forge-viewers

`crates/forge-viewers` — Forge file viewers: image metadata for the image editor and CSV/TSV tables for Edit as Table (pure, also built to WebAssembly).

## Purpose

Data behind Forge's non-text file editors, computed in Rust so the desktop app and the
browser mode (WebAssembly) share it:

- `image`: format, dimensions and colour depth of PNG, JPEG, GIF, WebP, ICO, BMP and
  SVG files for the image editor, and their bytes as a data URL payload.
- `table`: CSV/TSV parsing and writing for "Edit as Table" that keeps the delimiter,
  the quoting of every cell and the final line break, plus the table edits and sorting.

## Modules

| Module | Summary |
| --- | --- |
| `image.rs` | Image metadata for the image editor (PHPStorm shows "512x512 PNG (32-bit color) 4.17 kB"). Raster formats through `imagesize`, SVG from the root element's `width`/`height`/`viewBox`. |
| `table.rs` | CSV/TSV for "Edit as Table". The parser remembers which cells were quoted, so writing an unchanged table gives back the same text, and edits only touch the cells they change. |

## Main public items

```rust
pub use image::{image_data, image_info, is_image_path, ImageData, ImageInfo};
pub use table::{apply_edits, detect_delimiter, parse_table, sort_order, CsvEdit, CsvTable};
```

## Used by

- IPC: `image_read`, `csv_parse`, `csv_edit`, `csv_sort_order` (`app/src-tauri/src/commands/viewers.rs`)
- UI: `app/ui/src/features/images/`, `app/ui/src/features/csv/`
- Browser mode: built into `forge-wasm` (`viewers.rs`)

## Dependencies

- Workspace crates: none
- External: `base64`, `imagesize` (MIT), `regex-lite`, `serde`, `serde_json`

## Tests

3 source files; 10 unit tests (round trips of quoting, delimiters and line endings, edits,
sorting, PNG/GIF/BMP headers, SVG sizes).

```sh
cargo test -p forge-viewers
```
