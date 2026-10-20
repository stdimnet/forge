---
title: forge-core
description: "Forge IDE core: workspace, file system, settings and event bus."
order: 50
tags: [crate, rust]
---

# forge-core

`crates/forge-core` — Forge IDE core: workspace, file system, settings and event bus.

## Purpose

Forge core: the language-agnostic foundation every pack builds on.

## Modules

| Module | Summary |
| --- | --- |
| `error.rs` |  |
| `gitignore.rs` | Which project files git ignores, so the project tree can gray them out like PHPStorm. |
| `search.rs` | Fuzzy file search for "Go to File" / Search Everywhere. |
| `settings.rs` | Layered settings: built-in defaults < user < project. |
| `watch.rs` | Watches the open project for changes made outside the editor (git checkouts, composer, code generators, other editors) and reports them in debounced batches. |
| `workspace.rs` |  |

## Main public items

```rust
pub use error::{ForgeError, Result};
pub use search::{fuzzy_match, fuzzy_match_with, FileMatch, FuzzyMatch, MatchOptions};
pub use settings::{Settings, SettingsChange, SettingsScope};
pub use watch::{FsChanges, ProjectWatcher};
pub use workspace::{FileEntry, FileKind, Workspace};
```

## Used by

- IPC: `settings_get_all`, `settings_set`, `workspace_open`, `workspace_current`, `fs_list_dir`, `fs_read_file`, `fs_write_file`, `fs_create_file`, `fs_create_dir`, `fs_rename`, `fs_delete` (through the backend); events `settings://changed`, `fs://changed`
- UI: `app/ui/src/state/settings.ts`, `app/ui/src/state/workspace.ts`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-fuzzy`](forge-fuzzy.md)
- External: `dirs`, `ignore`, `notify`, `serde`, `serde_json`, `thiserror`, `toml`, `tracing`

## Tests

7 source files, about 1406 lines; 24 unit tests.

```sh
cargo test -p forge-core
```
