---
title: forge-localhistory
description: "Local History: file contents recorded on save, refactorings, VCS operations and external changes, with labels, revert and selection history."
order: 138
tags: [crate, rust, local history]
---

# forge-localhistory

`crates/forge-localhistory` — Local History: file contents recorded on save, refactorings, VCS operations and external changes, with labels, revert and selection history.

## Purpose

The model and store behind PHPStorm's Local History ([user guide](../../user/local-history.md)).

- **Event model** (`model`): a `ChangeSet` (id, time, `ChangeKind`, name) holds `Entry`s, one
  per path, with the path's `FileState` before and after (`Unknown`, `Absent`, `Dir` or
  `File(hash)`; renames and moves carry `from`). Labels are change sets without entries;
  `Baseline` sets keep the content a file had when Forge first read it and are never listed.
  `Limits` decide what is recorded (excluded folders, size limit, no binary files).
- **Storage** (`storage`): `Storage` is implemented in memory (`MemStorage`, used by
  WebAssembly and tests) and on disk (`DiskStorage`, feature `disk`): an append-only
  `changes.jsonl` (a set written again replaces the earlier line; compacted on load) and
  `content/<2 hex>/<hash>`, zlib-compressed and deduplicated by the first 128 bits of the
  SHA-256 of the content.
- **`LocalHistory`**: `record` (unknown "before" states come from the last recorded state;
  changes of one operation share a `group` and are merged into one set), `baseline`
  (detects external changes), `put_label`, and the queries: `history` (file revisions
  following renames, or folder revisions, with labels and a name/content filter), `state_at` /
  `content` (a revision is the state *before* its change set), `folder_changes`,
  `revert_file` / `revert_folder` / `revert_change` (plans of writes, deletes and folders to
  create, in a safe order), `selection_history` (lines followed back through the revisions with
  a line diff), `recent_changes` (saves in a row grouped), `change_files`, `patch` (git-style
  unified diff) and `purge` (keep the last N working days, then drop the oldest sets until the
  contents fit the size limit, delete unreferenced contents).

The crate does no file system work on the project: callers hand it contents and apply the
revert plans through the project backend.

## Main public items

```rust
pub enum FileState { Unknown, Absent, Dir, File(String) }
pub enum ChangeKind { Save, Edit, Refactoring, External, Vcs, Create, Delete, Rename, Move, Copy, Revert, Label, Baseline }
pub struct ChangeSet { … }
pub struct NewChange { … }
pub struct Limits { … }
pub trait Storage { … }
pub struct MemStorage { … }
pub struct DiskStorage { … }
pub struct LocalHistory<S: Storage> { … }
pub struct RevertPlan { … }
pub fn content_hash(bytes: &[u8]) -> String { … }
```

## Used by

- IPC: `local_history_list`, `local_history_content`, `local_history_folder_changes`,
  `local_history_selection`, `local_history_recent`, `local_history_change_files`,
  `local_history_put_label`, `local_history_patch`, `local_history_revert`,
  `local_history_record`, `local_history_begin` / `local_history_end`,
  `local_history_snapshot` (`app/src-tauri/src/commands/local_history.rs`). A recorder thread
  per project receives messages from the file commands (`fs_write_text`, `fs_write_file`,
  `fs_create_*`, `fs_rename`, `fs_delete`, `fs_copy`, `fs_read_text`), `code_apply_edit`,
  Replace in Files, file templates and the file watcher; stored in
  `<data dir>/forge/projects/<name>-<hash>/local-history/`.
- UI: `app/ui/src/features/localHistory` (history tabs, Recent Changes popup);
  `withLocalHistory` (`app/ui/src/ipc/localHistory.ts`) brackets refactorings and VCS
  operations.
- Browser mode: built into `forge-wasm` (`local_history_call`, in-memory store).

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`, `sha2`, `similar`, `flate2` (feature `disk`)

## Tests

5 source files; 14 unit tests (revisions before saves, exclusions, grouped operations, renames
and folder reverts with labels, selection history, baselines and external changes, Recent
Changes grouping, retention by working days and size, the disk log and compressed contents).

```sh
cargo test -p forge-localhistory
```
