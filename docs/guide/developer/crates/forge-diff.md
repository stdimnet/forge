---
title: forge-diff
description: "Forge IDE comparisons: folder diff (Compare Directories), pure and WebAssembly-friendly."
order: 65
tags: [crate, rust, diff, compare]
---

# forge-diff

`crates/forge-diff` — Forge IDE comparisons: folder diff (Compare Directories), pure and
WebAssembly-friendly.

## Purpose

The folder diff of PHPStorm's Compare Directories (see the
[user guide](../../user/git.md#compare-files-and-folders)). Text diffs of two files are
computed by the diff viewer itself.

- `compare_listings`: two listings (paths relative to the compared folders, a size and an
  FNV-1a content digest per file) → rows in tree order (folders first, children after their
  folder) with `equal`, `different`, `leftOnly` or `rightOnly`; a folder is equal when
  everything below it is.
- `list_tree` / `compare_trees`: read two folders on this machine (`.git`, `.hg`, `.svn`
  skipped, symbolic links compared by their target, at most `MAX_ENTRIES` per side). Files
  are hashed in 64 KiB blocks (`digest_reader`), never read whole; a file that cannot be read
  is `unreadable` and never compares as equal.

## Main public items

```rust
pub fn compare_listings(left: &Listing, right: &Listing) -> DirCompare
pub fn compare_trees(left: &Path, right: &Path) -> io::Result<DirCompare>
pub fn list_tree(root: &Path, limit: usize) -> io::Result<Listing>
pub struct DirDiffEntry { path, dir, depth, status: DiffStatus, left_size, right_size, left_modified, right_modified }
```

## Used by

- `forge-backend`: `Backend::compare_dirs` (local: `compare_trees`; remote: the agent call
  `CompareDirs`, protocol 9) and `compare::copy_entry` (Copy to Left / Right)
- IPC: `compare_dirs`, `compare_copy` (`app/src-tauri/src/commands/compare.rs`)
- Browser mode: `compare_dir_listings` in `forge-wasm` on the demo files

## Dependencies

- Workspace crates: none
- External: `serde`

## Tests

Unit tests with fixture folders (every state, nesting, file against folder, limits).

```sh
cargo test -p forge-diff
```
