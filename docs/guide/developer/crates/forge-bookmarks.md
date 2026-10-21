---
title: forge-bookmarks
description: "Bookmarks: lists, mnemonics and line tracking through edits (pure, WebAssembly-friendly)."
order: 25
tags: [crate, rust]
---

# forge-bookmarks

`crates/forge-bookmarks` — Bookmarks: lists, mnemonics and line tracking through edits (pure, WebAssembly-friendly).

## Purpose

The model behind PHPStorm's bookmarks (`F11`, `Ctrl+F11`, the Bookmarks tool window `Alt+2`).

- `Bookmarks` holds named bookmark lists; one of them is the default list that `F11` adds
  to. A bookmark marks a line of a project-relative file and may carry a mnemonic (`0`–`9`,
  `A`–`Z`, unique across all lists) and a description.
- Every change is an `Op` applied with `Bookmarks::apply` (toggle, set mnemonic, describe,
  move between lists, add/rename/remove lists, …), so the desktop app and browser mode share
  the logic.
- Line positions follow edits: the editor reports each document change as `LineEdit`s in
  old-document coordinates and `Op::ApplyEdits` moves the bookmarks below them; a bookmark on
  a removed line stays where the change happened. Renamed and deleted files are
  followed with `Op::RenamePath` / `Op::RemovePath`.

The crate has no I/O; the state is serialized as JSON by the caller.

## Main public items

```rust
pub const DEFAULT_LIST: &str = "Bookmarks";
pub struct Bookmark { … }
pub struct BookmarkList { … }
pub struct Bookmarks { … }
pub struct LineEdit { … }
pub enum Op { … }
pub fn normalize_mnemonic(c: char) -> Option<char> { … }
pub fn map_line(line: usize, edit: &LineEdit) -> usize { … }
```

## Used by

- IPC: `bookmarks_get`, `bookmarks_apply` (`app/src-tauri/src/commands/workbench.rs`; stored in
  `<data dir>/forge/projects/<name>-<hash>/bookmarks.json`, never in the project)
- UI: `app/ui/src/features/bookmarks` (tool window, gutter icons, mnemonic popup)
- Browser mode: built into `forge-wasm` (`bookmarks_apply`)

## Dependencies

- Workspace crates: none
- External: `serde`

## Tests

1 source file, about 570 lines; 7 unit tests (toggling, mnemonics, lists, line tracking
through inserted and removed lines, renames).

```sh
cargo test -p forge-bookmarks
```
