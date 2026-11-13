---
title: forge-plugin-api
description: "Stable plugin API for Forge language packs and feature plugins."
order: 180
tags: [crate, rust]
---

# forge-plugin-api

`crates/forge-plugin-api` — Stable plugin API for Forge language packs and feature plugins.

## Purpose

The contract between Forge and its plugins.

A plugin (a language pack like PHPForge, or a feature like Git) describes itself with a
`PluginManifest` and contributes capabilities through `Contributions`: commands,
keybindings, tool windows, typed settings, language servers, run configuration types, test
frameworks of the shared test runner (`testFrameworks`, defined by
[`forge-testing`](forge-testing.md)), inspections and the mapping of external tools' problems
onto them (`inspections`, `inspectionMappings`, defined by
[`forge-inspections`](forge-inspections.md); `PluginRegistry::inspection_registry`) and AI
providers/agents/generators. The `PluginRegistry` tracks installed plugins, their
enablement, and answers lookups over the contributions of enabled plugins. Packs depend
only on this crate, never on app internals.

## Modules

| Module | Summary |
| --- | --- |
| `contributions.rs` | Contribution types: everything a plugin can add to the IDE. |
| `lifecycle` | Re-exports the project lifecycle of `forge_core::lifecycle` (`ProjectLifecycle`, `ProjectLifecycleBus`, `Objection`, …) for packs whose Rust services follow the window's project; the desktop app subscribes them (`app/src-tauri/src/lifecycle.rs`). |
| `profile.rs` | Launch profiles: which packs a Forge process loads (`phpforge`, `--packs …`, `FORGE_PACKS`). Detection from the command line, the environment and the program name, and which plugins a profile turns off. |
| `registry.rs` | The plugin registry: which plugins are installed, which are enabled, and lookups over the contributions of the enabled ones. |
| `settings_model.rs` | The model behind the Settings dialog (Ctrl+Alt+S): the page tree built from registered pages, search over pages, settings and plugins, and highlighting of search hits. The UI sends what it shows and renders the answer; this module decides what matches. |

## Main public items

```rust
pub use contributions::*;
pub use profile::{LaunchProfile, ProfileView};
pub use registry::{PluginDescriptor, PluginInfo, PluginRegistry};
pub struct PluginManifest { … }
pub trait Plugin: Send + Sync { … }
```

## Used by

- IPC: `settings_schema`, `settings_search`, `settings_validate`, `plugins_list`, `plugin_set_enabled`
- UI: `app/ui/src/features/settings`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-filetypes`](forge-filetypes.md), [`forge-inspections`](forge-inspections.md), [`forge-keymap`](forge-keymap.md)
- External: `serde`, `serde_json`, `thiserror`, `toml`

## Tests

4 source files, about 1495 lines; 18 unit tests.

```sh
cargo test -p forge-plugin-api
```
