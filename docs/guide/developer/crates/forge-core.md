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
| `project_files.rs` | Project-relative files that Forge itself keeps in a project (`.forge/settings.toml`, `.forge/dataSources.toml`, `.forge/breakpoints.json`, ...) and project files it reads (`.env`), wherever the project lives: `LocalFiles` for a folder on this machine, the project backend (`forge_backend::BackendFiles`) for a project on a remote host. |
| `search.rs` | Fuzzy file search for "Go to File" / Search Everywhere, and Find in Files. |
| `settings.rs` | Layered settings: built-in defaults < user < Settings for New Projects (`SettingsScope::NewProjects`, `new-project-settings.toml`) < project. `Settings::set_many` stores several values and saves each layer once (the Code Style schemes); `parse_settings_toml` / `settings_to_toml` convert between flat keys and the nested files (settings archives). |
| `watch.rs` | Watches the open project for changes made outside the editor (git checkouts, composer, code generators, other editors) and reports them in debounced batches. |
| `workspace.rs` |  |

## Main public items

```rust
pub use error::{ForgeError, Result};
pub use project_files::{LocalFiles, MemoryFiles, ProjectFiles, SharedFiles};
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

- Workspace crates: [`forge-find`](forge-find.md), [`forge-fuzzy`](forge-fuzzy.md)
- External: `dirs`, `ignore`, `notify`, `serde`, `serde_json`, `thiserror`, `toml`, `tracing`

## Tests

8 source files, about 1689 lines; 27 unit tests.

```sh
cargo test -p forge-core
```
