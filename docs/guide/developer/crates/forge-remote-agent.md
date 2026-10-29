---
title: forge-remote-agent
description: "Headless Forge agent: serves a project's files, git, processes, language servers and PHP index over stdio for remote development."
order: 220
tags: [crate, rust]
---

# forge-remote-agent

`crates/forge-remote-agent` — Headless Forge agent: serves a project's files, git, processes, language servers and PHP index over stdio for remote development.

## Purpose

The Forge agent: a headless `LocalBackend` behind the agent
`protocol`, run on a remote host over SSH (stdin/stdout).

The desktop app uploads this binary to `~/.cache/forge-agent/<version>/`, starts
`forge-remote-agent serve` and talks to it with `forge_backend::rpc::RpcClient`. Every
service runs next to the project's files: file system, file search, git, terminals,
processes, language servers (the PHP server indexes the remote project itself) and the
PHP index. The agent links the built-in PHP language server php-lsp
([phpforge-builtin-lsp](phpforge-builtin-lsp.md)): `forge-remote-agent --php-lsp --stdio`
runs it, and the PHP pack's config on the host starts the agent binary that way.

## Modules

| Module | Summary |
| --- | --- |
| `browse.rs` | Listing absolute directories for the IDE's remote folder chooser. |

## Main public items

```rust
pub use browse::browse;
pub fn version_line() -> String { … }
pub async fn serve<R, W>(reader: R, writer: W) -> Result<()>
```

## Used by

- IPC: none: it is the server side of the agent protocol, started on remote hosts
- UI: —
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-backend`](forge-backend.md), [`forge-core`](forge-core.md), [`forge-lsp`](forge-lsp.md), [`forge-process`](forge-process.md), [`forge-project`](forge-project.md), [`forge-vcs`](forge-vcs.md)
- External: `dirs`, `serde`, `serde_bytes`, `serde_json`, `tokio`, `tracing`, `tracing-subscriber`

## Tests

3 source files, about 668 lines; 2 unit tests and 13 integration tests in `tests/` (`agent.rs`, `binary.rs`).

```sh
cargo test -p forge-remote-agent
```
