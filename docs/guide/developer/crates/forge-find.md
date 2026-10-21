---
title: forge-find
description: "Find in Files: query compilation, file masks and line matching (pure, WebAssembly-friendly)."
order: 90
tags: [crate, rust]
---

# forge-find

`crates/forge-find` — Find in Files: query compilation, file masks and line matching (pure, WebAssembly-friendly).

## Purpose

Find in Files (PHPStorm's Ctrl+Shift+F): the search request, file masks and per-line
matching. The crate is pure so the desktop backend (walking the project with
`forge-core`), the remote agent and the browser-mode WebAssembly build share it.

- The query is a literal or a regular expression (`regex` syntax), optionally
  case-sensitive and limited to whole words.
- The file mask is a comma-separated list of globs (`*.php, *.twig`); `!` excludes
  (`!*Test.php`). A glob without `/` matches the file name, otherwise the whole path.
- Matches are reported per line, like PHPStorm's result list: the line text (trimmed and
  cut around the first match) with every match range in it. Columns and ranges are UTF-16
  offsets so the UI can use them directly.

## Main public items

```rust
pub struct FindRequest { … }
pub struct LineMatch { … }
pub struct FileMatches { … }
pub struct FindResult { … }
pub struct FileMask { … }
pub fn glob_match(glob: &str, text: &str) -> bool { … }
pub struct Finder { … }
pub fn is_binary(bytes: &[u8]) -> bool { … }
pub struct Collector<'a> { … }
pub fn find_in<'t>(
```

## Used by

- IPC: `find_in_files` (`app/src-tauri/src/commands/workspace.rs`, a `Backend` method, agent call `FindInFiles`)
- UI: `app/ui/src/features/find`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `regex`, `serde`

## Tests

1 source files, about 493 lines; 5 unit tests.

```sh
cargo test -p forge-find
```
