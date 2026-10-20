---
title: forge-debug
description: "Forge IDE debugger: DBGp (Xdebug 3) server, path mappings and breakpoints."
order: 70
tags: [crate, rust]
---

# forge-debug

`crates/forge-debug` — Forge IDE debugger: DBGp (Xdebug 3) server, path mappings and breakpoints.

## Purpose

Forge's PHP debugger: a DBGp server for Xdebug 3.

- `DebugServer` listens for Xdebug (default port 9003), accepts any number of
  connections and turns each into a `Session`.
- Sessions identify their server (`PHP_IDE_CONFIG`, `SERVER_NAME`), map server paths to
  project files (`PathMapper`), set the project's breakpoints and report state changes
  as `DebugEvent`s.
- Breakpoints live in `<project>/.forge/breakpoints.json`, servers with their path
  mappings in `<project>/.forge/php-servers.json`.

## Modules

| Module | Summary |
| --- | --- |
| `breakpoints.rs` | Project breakpoints, persisted in `<project>/.forge/breakpoints.json`. |
| `config.rs` |  |
| `connection.rs` | One DBGp connection: writes commands with transaction ids and routes the engine's answers back to the waiting caller. Packets that are not answers (`stream`, `notify`) go to a callback. |
| `error.rs` |  |
| `events.rs` |  |
| `mapping.rs` | Server ↔ local path mappings (PHPStorm's "Servers > Use path mappings"). |
| `protocol.rs` | DBGp wire format and typed views of the engine's answers. |
| `server.rs` | The DBGp listener ("Start Listening for PHP Debug Connections") and the registry of sessions, breakpoints and servers it shares with them. |
| `servers.rs` | PHP servers (PHPStorm's "Settings > PHP > Servers"): which host a debug connection comes from and how its paths map into the project. Stored per project in `<project>/.forge/php-servers.json`. |
| `session.rs` | A debug session: one script run connected through DBGp. |
| `xml.rs` | A tiny DOM for DBGp messages. Responses are small, so building a tree is simpler than streaming and keeps the typed parsers in `protocol.rs` readable. |

## Main public items

```rust
pub use breakpoints::{…};
pub use config::DebugConfig;
pub use error::{DebugError, DebugResult};
pub use events::{BreakReason, DebugEvent, EventSink, Location, SessionInfo, SessionStatus};
pub use mapping::{PathMapper, PathMapping};
pub use protocol::{ContextName, Property, RemoteBreakpoint};
pub use server::DebugServer;
pub use servers::{ServerConfig, ServerIdentity};
pub use session::{Frame, Session, StepKind};
pub fn launch_env(port: u16, server_name: Option<&str>) -> Vec<(String, String)> { … }
```

## Used by

- IPC: `debug_status`, `debug_listen_start`, `debug_listen_stop`, `debug_sessions`, `debug_accept_connection`, `debug_ignore_connection`, `debug_step`, `debug_pause`, `debug_stop`, `debug_detach`, `debug_run_to_cursor`, `debug_stack`, `debug_contexts`, `debug_variables`, `debug_property`, `debug_eval`, `debug_set_value`, `debug_breakpoints`, `debug_breakpoint_add`, `debug_breakpoint_update`, `debug_breakpoint_remove`, `debug_breakpoint_toggle`, `debug_breakpoints_mute`, `debug_watches`, `debug_watches_set`, `debug_servers`, `debug_servers_save`, `debug_launch_env` (`app/src-tauri/src/commands/debug.rs`)
- UI: `app/ui/src/features/debug`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md)
- External: `base64`, `quick-xml`, `serde`, `serde_json`, `thiserror`, `tokio`, `tracing`

## Tests

12 source files, about 3005 lines; 15 unit tests and 7 integration tests in `tests/` (`dbgp_server.rs`).

```sh
cargo test -p forge-debug
```
