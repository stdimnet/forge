---
title: forge-find
description: "Find in Files and TODO scanning: query compilation, file masks, line matching and comment patterns (pure, WebAssembly-friendly)."
order: 90
tags: [crate, rust]
---

# forge-find

`crates/forge-find` — Find in Files and TODO scanning: query compilation, file masks, line matching and comment patterns (pure, WebAssembly-friendly).

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

The module `todo` scans for the TODO tool window: `TODO` / `FIXME` style patterns (regular
expressions, PHPStorm's `\btodo\b.*` and `\bfixme\b.*` by default) inside comments only.
`CommentSyntax::for_path` knows the line and block comments and strings of each file type
(PHP, Blade, Twig, C-like languages, markup, CSS/SCSS, hash-comment languages, SQL, Lua);
plain text and Markdown are scanned as a whole. `TodoRequest` can limit the scan to some
`paths` and pass unsaved editor `buffers` that replace the files on disk.

A request may carry a `scope` (`forge_scopes::ScopeFilter`: a predefined scope or a scope
pattern of the Scopes page with its context); `Finder::wants` applies it with the directory,
the path list and the file mask, so the index, the directory walk, the agent and Replace in
Files all honor it.

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
pub mod todo; // TodoRequest, TodoPattern, TodoItem, TodoFile, TodoResult, TodoScanner,
              // TodoCollector, CommentSyntax, scan_todos
```

## Used by

- IPC: `find_in_files` (`app/src-tauri/src/commands/workspace.rs`, a `Backend` method, agent call `FindInFiles`)
  and `todo_scan` (`app/src-tauri/src/commands/workbench.rs`, `Backend::find_todos`, agent call `FindTodos`)
- UI: `app/ui/src/features/find`, `app/ui/src/features/todo`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: `forge-scopes`
- External: `regex`, `serde`

## Tests

2 source files, about 1080 lines; 10 unit tests.

```sh
cargo test -p forge-find
```
