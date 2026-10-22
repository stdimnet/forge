---
title: forge-lsp
description: "Forge IDE: generic Language Server Protocol client and server manager."
order: 140
tags: [crate, rust]
---

# forge-lsp

`crates/forge-lsp` — Forge IDE: generic Language Server Protocol client and server manager.

## Purpose

Forge's generic Language Server Protocol client.

- `framing`: `Content-Length` JSON-RPC framing over stdio.
- `LspClient`: one server process; request/response correlation, notifications,
  answers to server→client requests, document sync and typed feature requests.
- `LspManager`: routes files to servers by extension and starts them lazily; several
  servers may serve one file and their results are merged.
- `model`: editor-friendly result types consumed by the UI.
- `launch`: how server processes start (`LocalLauncher` here, or on a remote host).

Nothing here is language specific; language packs contribute `LspServerConfig`s.
`LspClient::update_settings` merges later changes (the code style options before Reformat
Code, `LspManager::format`'s `settings`) into a server's settings, pushes them with
`workspace/didChangeConfiguration` and answers later `workspace/configuration` requests with
them.

## Modules

| Module | Summary |
| --- | --- |
| `client.rs` | An LSP client talking to one language server process over stdio. |
| `config.rs` | Language server configuration as contributed by language packs. |
| `framing.rs` | JSON-RPC message framing used by LSP over stdio: a `Content-Length` header block, an empty line, then exactly that many bytes of UTF-8 JSON. |
| `hierarchy.rs` | Type and Call Hierarchy from the servers as `forge_codeintel::hierarchy`'s model: `LspManager::type_graph` (prepare, then supertypes and subtypes breadth-first, at most 300 types), `call_root` and `call_nodes` (incoming / outgoing calls of an item); items keep the server id and the server's own item as their `handle`. The manager's `prepare_type_hierarchy`, `type_hierarchy`, `prepare_call_hierarchy`, `incoming_calls` and `outgoing_calls` ask only servers announcing `typeHierarchyProvider` / `callHierarchyProvider` (`Feature::TypeHierarchy`, `Feature::CallHierarchy`). |
| `launch.rs` | Starting language server processes. `LocalLauncher` spawns them on this machine; other launchers start them elsewhere (on a remote development host) and hand back their stdio as streams, so `LspClient`(crate::LspClient) works the same either way. |
| `manager.rs` | Routes files to language servers: one server per registered config, started lazily the first time a matching file is used. |
| `model.rs` | Editor-friendly views of LSP results. |

## Main public items

```rust
pub use client::{path_to_uri, uri_to_path, LspClient, LspEvent, ServerState};
pub use config::{find_on_path, LspServerConfig};
pub use launch::{LocalLauncher, ServerIo, ServerLauncher, ServerProcess};
pub use lsp_types;
pub use manager::{LspManager, ServerStatus, COMPLETION_SERVER_KEY};
```

## Used by

- IPC: `lsp_did_open`, `lsp_did_change`, `lsp_did_save`, `lsp_did_close`, `lsp_hover`, `lsp_completion`, `lsp_completion_resolve`, `lsp_definition`, `lsp_references`, `lsp_document_symbols`, `lsp_signature_help`, `lsp_format`, `lsp_status`, `lsp_register_server` (`app/src-tauri/src/commands/lsp.rs`)
- UI: `app/ui/src/features/lsp`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md)
- External: `futures-util`, `lsp-types`, `serde`, `serde_json`, `tokio`, `tracing`

## Tests

7 source files, about 2596 lines; 17 unit tests and 4 integration tests in `tests/` (`fake_server.rs`).

```sh
cargo test -p forge-lsp
```
