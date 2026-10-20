---
title: forge-langservers
description: "Forge IDE: catalog, detection, configuration and installation of language servers."
order: 120
tags: [crate, rust]
---

# forge-langservers

`crates/forge-langservers` — Forge IDE: catalog, detection, configuration and installation of language servers.

## Purpose

Forge's catalog of language servers for many languages, and everything around it:

- `catalog`: the data-driven catalog (`catalog.toml`) of languages and servers with
  commands, initialization options, settings, install hints and licences,
- `detect`: finding installed servers (PATH, npm/nvm/volta, pip/pipx, cargo, go,
  dotnet, gem, Mason, Windows `.cmd` shims),
- `scan`: which languages a project uses,
- `config`: building a `forge_lsp::LspServerConfig` for a project,
- `plan`: which servers start, and the rows of the settings page,
- `install`: running an install command with streamed output,
- `licence`: the Intelephense licence key in the OS keychain.

PHP servers (Intelephense, phpactor) stay in the PHP pack; the app lists them next to
these rows.

## Modules

| Module | Summary |
| --- | --- |
| `catalog.rs` | The built-in catalog (`catalog.toml`): languages Forge detects in projects and the language servers that serve them. |
| `config.rs` | Turning a catalog entry into a `LspServerConfig` for one project: file routing, user overrides and the project specific bits some servers need (TypeScript SDK, the Vue TypeScript plugin, ESLint's workspace folder, jdtls' data directory, Lua runtime). |
| `detect.rs` | Finding installed language servers: `PATH` plus the places package managers put executables that are often missing from `PATH` in a desktop session (npm global prefix, nvm/fnm/volta, `~/.local/bin` for pip/pipx, `~/.cargo/bin`, `~/go/bin`, `~/.dotnet/tools`, gem user dirs and Neovim's Mason directory). |
| `install.rs` | Running an install command (`npm install …`, `pipx install …`) with streamed output. Commands come from the catalog only and run without a shell. |
| `licence.rs` | Intelephense licence key: kept in the OS keychain (service `forge-lsp`, account `intelephense`), never in settings files. |
| `plan.rs` | Which servers to start for a project, and the rows of the Language Servers settings page. |
| `prefs.rs` | User preferences for language servers, read from Forge settings. |
| `scan.rs` | Quick scan of a project: which catalog languages occur (by file name, honouring `.gitignore`, capped), which root files exist and which npm packages it depends on. |

## Main public items

```rust
pub use catalog::{Catalog, InstallOption, Language, ServerSpec};
pub use config::{build_config, merge_json, ConfigContext};
pub use detect::{DetectEnv, Detector};
pub use plan::{plan, rows, Decision, LiveState, RowStatus, ServerRow, Skip};
pub use prefs::{server_key, Prefs, ServerPrefs};
pub use scan::{scan_project, ProjectScan};
```

## Used by

- IPC: `langservers_list`, `langservers_apply`, `langservers_install`, `langservers_cancel_install`, `lsp_workspace_symbols`, `intelephense_licence_status`, `intelephense_set_licence` (`app/src-tauri/src/commands/langservers.rs`)
- UI: `app/ui/src/features/langservers`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-lsp`](forge-lsp.md)
- External: `dirs`, `ignore`, `keyring`, `serde`, `serde_json`, `tokio`, `toml`, `tracing`

## Tests

9 source files, about 3195 lines; 35 unit tests.

```sh
cargo test -p forge-langservers
```
