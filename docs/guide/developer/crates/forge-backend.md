---
title: forge-backend
description: "Forge IDE project backends: the Backend trait, the local backend and the remote agent protocol."
order: 30
tags: [crate, rust]
---

# forge-backend

`crates/forge-backend` — Forge IDE project backends: the Backend trait, the local backend and the remote agent protocol.

## Purpose

Project backends: where an open project's files, git, processes, language servers and
PHP index live.

- `Backend` is what the app's workspace, file system, git, terminal, run, LSP and PHP
  commands talk to. The UI never knows which implementation answers.
- `LocalBackend` serves a folder on this machine.
- `RemoteBackend` serves a folder on another machine through the Forge agent
  (`crates/forge-remote-agent`), which is itself a `LocalBackend` behind the
  `protocol`. `rpc::RpcClient` multiplexes calls, events and byte channels over any
  stream (SSH in `crates/forge-remote`, an in-memory pipe in tests).

## Modules

| Module | Summary |
| --- | --- |
| `local.rs` | A project folder on this machine. |
| `paths.rs` | Path helpers that do not touch the file system (the project may live on another machine). |
| `php.rs` | The PHP index of a project and the PHP language server lookup, wherever the project lives. Offsets are UTF-16 (JS strings), see `phpforge_lang_php::service`. |
| `protocol.rs` | The Forge agent protocol: MessagePack frames over a byte stream (the agent's stdio). |
| `remote.rs` | A project on another machine, served by the Forge agent over an `RpcClient`. |
| `rpc.rs` | The client end of the agent `protocol`(crate::protocol): calls with out-of-order answers, pushed events and byte channels over one stream pair. |

## Main public items

```rust
pub use local::LocalBackend;
pub use php::PhpBackend;
pub use remote::RemoteBackend;
pub struct RemoteHost { … }
pub struct BackendInfo { … }
pub struct TerminalSpec { … }
pub trait Terminal: Send + Sync { … }
pub trait Process: Send + Sync { … }
pub trait Backend: Send + Sync { … }
pub fn folder_name(root: &Path) -> String { … }
```

## Used by

- IPC: every project command (workspace, fs, git, terminal, run, php, lua) goes through `AppState::backend()`
- UI: `app/ui/src/state/workspace.ts`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-lsp`](forge-lsp.md), [`forge-process`](forge-process.md), [`forge-project`](forge-project.md), [`forge-vcs`](forge-vcs.md), [`phpforge-lang-php`](phpforge-lang-php.md)
- External: `futures-util`, `rmp-serde`, `serde`, `serde_bytes`, `serde_json`, `tokio`, `tracing`

## Tests

7 source files, about 2146 lines; 10 unit tests.

```sh
cargo test -p forge-backend
```
