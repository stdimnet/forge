---
title: forge-filetypes
description: "File type recognition for Forge: names, extensions, shebangs and template data languages (pure, WebAssembly-friendly)."
order: 80
tags: [crate, rust]
---

# forge-filetypes

`crates/forge-filetypes` — File type recognition for Forge: names, extensions, shebangs and template data languages (pure, WebAssembly-friendly).

## Purpose

File type recognition (PHPStorm's Settings | Editor | File Types): which language a file
is, what the status bar calls it, its indentation and, for template languages, the data
language between the template tags.

The bundled types (`builtin_file_types`) are extended or replaced by the file types of
plugin manifests (`[[plugin.contributions.fileTypes]]`). Detection prefers, in order:
file names and path patterns, the longest matching extension (so `x.blade.php` beats
`php`), and the interpreter of a `#!` line. Later registrations win ties.

The crate is pure so the desktop app (IPC) and the browser-mode UI (WebAssembly) share it.
The UI only maps a file type id to its CodeMirror syntax mode.

## Modules

| Module | Summary |
| --- | --- |
| `builtin.rs` | File types every Forge installation knows (PHPStorm's bundled file types). Plugins add more, or replace one of these by contributing a file type with the same id, through `[[plugin.contributions.fileTypes]]` in their manifests. |
| `data.rs` | Template Data Languages (as in PHPStorm): the language of the content *between* template tags. |
| `glob.rs` | The two pattern flavours of file type settings. |

## Main public items

```rust
pub use builtin::builtin_file_types;
pub use data::{data_language_for, DataLanguage, DATA_LANGUAGES};
pub use glob::{matches_glob_list, wildcard_match};
pub struct FileTypeSpec { … }
pub struct FileTypeRegistry { … }
pub struct DetectRequest { … }
pub struct FileRef { … }
pub struct FileTypeInfo { … }
```

## Used by

- IPC: `filetypes_detect`
- UI: `app/ui/src/editor/fileTypes.ts`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `serde`

## Tests

5 source files, about 1004 lines; 12 unit tests.

```sh
cargo test -p forge-filetypes
```
