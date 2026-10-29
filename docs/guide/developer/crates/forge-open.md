---
title: forge-open
description: "Forge IDE open requests: forge:// links, the CLI launcher and the Browser Companion local server."
order: 160
tags: [crate, rust]
---

# forge-open

`crates/forge-open` — Forge IDE open requests: forge:// links, the CLI launcher and the Browser Companion local server.

## Purpose

Opening files in Forge from the outside world:

- `request`: open requests and `forge://open?file=…&line=…` links,
- `cli`: the `forge path/to/file:12` launcher,
- `resolve`: which project a file belongs to (open, recent, or none),
- `symbols`: find a component/class declaration by name,
- `server` + `pairing`: the Browser Companion's local HTTP API,
- `project`: project detection and dotenv edits for framework integrations.

## Modules

| Module | Summary |
| --- | --- |
| `cli.rs` | Command-line launcher: `forge [project-dir]`, `forge path/to/file:12[:3]`, `forge --line 12 --column 3 file` (JetBrains style), `forge file 12 3` (launch-editor) and `forge forge://open?...`. |
| `http.rs` | A deliberately tiny HTTP/1.1 subset for the local Browser Companion API: one request per connection, `Content-Length` bodies only, small hard limits. |
| `live_edit.rs` | Live Edit: which files swap style sheets (`.css`) or reload pages, and the `LiveEditHub` of events paired browsers poll with `GET /api/forge/live-edit?since=<seq>`. |
| `pairing.rs` | Pairing browsers with Forge. |
| `project.rs` | What the Browser Companion settings page needs to know about the open project to show the relevant framework snippets first, and the one config change it may apply itself: setting a variable in a dotenv file. |
| `request.rs` | Open requests as they arrive from the browser extension, `forge://` links and the CLI, plus their validation. |
| `resolve.rs` | Map a requested file to a project: the open one, a recent one that has to be opened first, or none (the user has to confirm opening it). |
| `server.rs` | The Browser Companion's local HTTP API, modelled on JetBrains' built-in server. |
| `symbols.rs` | Find the declaration of a symbol the browser only knows by name (an Angular component's constructor name, a React component without debug source) in the open project. |

## Main public items

```rust
pub use request::{LinkAction, OpenRequest, RequestError, SymbolKind, SymbolRequest};
pub use resolve::{resolve_file, KnownProject, ProjectContext, ResolveError, Target};
pub enum OpenSource { … }
pub struct OpenEvent { … }
pub fn open_event(
```

## Used by

- IPC: `companion_status`, `companion_start_pairing`, `companion_cancel_pairing`, `companion_revoke_client`, `companion_ui_ready`, `companion_open_link`, `companion_project_profile`, `companion_set_env_var` (`app/src-tauri/src/commands/companion.rs`)
- UI: `app/ui/src/features/browserCompanion`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md)
- External: `dirs`, `getrandom`, `percent-encoding`, `serde`, `serde_json`, `sha2`, `thiserror`, `tokio`, `tracing`

## Tests

9 source files, about 2862 lines; 40 unit tests.

```sh
cargo test -p forge-open
```
