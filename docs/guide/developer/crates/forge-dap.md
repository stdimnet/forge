---
title: forge-dap
description: "Forge IDE shared debugging layer: Debug Adapter Protocol client, adapter registry (Rust trait and manifest adapters), sessions bridged to the Debug tool window, native views and Attach to Process."
order: 69
tags: [crate, rust, debug, dap]
---

# forge-dap

`crates/forge-dap` — Forge IDE shared debugging layer: Debug Adapter Protocol client, adapter
registry and native debugger views.

## Purpose

The debugger of every language pack except PHP (which uses Xdebug through
[forge-debug](forge-debug.md)). It is a client of the
[Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) and maps DAP
sessions onto Forge's debugger model, so the Debug tool window drives Xdebug and DAP sessions
with the same `debug_*` commands and `debug://*` events.

- `client::DapClient` speaks the protocol: Content-Length framing (`codec`), requests with
  sequence numbers and timeouts, responses, events and reverse requests.
- `transport::Transport`: an adapter process on stdio, a TCP connection to an adapter that
  listens (started by Forge with `${port}`, or already running: "Attach to DAP server"), or a
  TCP server Forge opens for an adapter that connects back.
- `adapter`: `DebugAdapterDescriptor` (how to start an adapter and build its `launch` /
  `attach` arguments), `ManifestAdapter` (adapters declared with
  `[[contributions.debugAdapters]]`, request templates with `${program}`, `${args}`,
  `${cwd}`, `${env}`, `${pid}`, `${core}`, `${stopOnEntry}`), `AdapterRegistry` and
  `LaunchRequest` (launch, attach to a process, attach to a DAP server, core dump).
- `manager::DapManager` starts and tracks sessions, shares the project's breakpoints with
  Xdebug through a `BreakpointHost` (the app's `DebugServer`) and resends them when they
  change. Relative program and working directory paths are resolved against the project.
- `session::DapSession`: initialize → launch/attach → breakpoints → configurationDone (for
  adapters that announce `initialized` early, like GDB, breakpoints go first), stopped /
  continued / output / thread / breakpoint / terminated / exited / module / process / memory /
  capabilities / invalidated events, the reverse requests `runInTerminal` (Forge starts the
  program with its output in the console) and `startDebugging` (child sessions). Requests:
  threads, stackTrace, scopes, variables, setVariable / setExpression, evaluate (watch, repl,
  hover), completions, continue / next / stepIn / stepOut (also by instruction), pause, goto,
  restart, terminate, disconnect, setBreakpoints (conditions, hit counts, logpoints),
  setFunctionBreakpoints, setExceptionBreakpoints (filters stored as breakpoints),
  dataBreakpointInfo + setDataBreakpoints (watchpoints), readMemory / writeMemory,
  disassemble, modules, loadedSources.
- `views`: rows of the Memory view (hex/ASCII), Disassembly instructions, address parsing.
- `processes`: local processes for Attach to Process (`/proc` on Linux, `ps` on macOS,
  `tasklist` on Windows; no third-party crate).
- `fake`: a simulated adapter running a small C program, used by the tests and available as
  the `forge-dap-fake` binary (stdio, `--connect <port>`, `--listen <port>`).

## Modules

| Module | Summary |
| --- | --- |
| `adapter.rs` | Debug adapter descriptors: how to start an adapter and which `launch` / `attach` arguments it gets. |
| `client.rs` | One connection to a debug adapter: requests, responses, events and reverse requests. |
| `codec.rs` | DAP base protocol: `Content-Length` framed JSON messages. |
| `error.rs` | `DapError`, convertible to `ForgeError`. |
| `fake.rs` | The simulated adapter (`DEMO_SOURCE`, `FakeAdapter`, `serve`). |
| `manager.rs` | `DapManager`: adapter registry and running sessions; `BreakpointHost`, `DapConfig`. |
| `processes.rs` | Local processes for "Attach to Process…". |
| `protocol.rs` | DAP messages and the typed bodies Forge uses. |
| `session.rs` | A DAP session bridged to Forge's debugger model (`SessionInfo`, `Frame`, `Property`, `DebugEvent`). |
| `transport.rs` | Stdio, TCP connect and TCP listen transports. |
| `views.rs` | Memory rows, instructions and addresses of the native views. |

## Main public items

```rust
pub use adapter::{AdapterInfo, AdapterRegistry, DebugAdapterDescriptor, LaunchMode, LaunchRequest, ManifestAdapter, ResolvedLaunch};
pub use error::{DapError, DapResult};
pub use manager::{BreakpointHost, DapConfig, DapManager};
pub use processes::{list_processes, ProcessInfo};
pub use session::{DapSession, LoadedSource};
pub use transport::{AdapterCommand, Transport};
pub use views::{Instruction, MemoryBlock, MemoryRow};

pub trait DebugAdapterDescriptor: Send + Sync {
    fn info(&self) -> AdapterInfo;
    fn resolve(&self, request: &LaunchRequest) -> DapResult<ResolvedLaunch>;
}
```

## Used by

- IPC: `dap_adapters`, `dap_start`, `dap_restart`, `dap_threads`, `dap_goto`, `dap_registers`,
  `dap_read_memory`, `dap_write_memory`, `dap_disassemble`, `dap_modules`,
  `dap_loaded_sources`, `dap_data_breakpoint`, `dap_processes`
  (`app/src-tauri/src/commands/dap.rs`); the shared `debug_*` commands route `dap-<n>`
  session ids here (`app/src-tauri/src/commands/debug.rs`).
- UI: `app/ui/src/features/debug` (`dapApi.ts`, `adapters.ts`, `nativeViews.tsx`,
  `AttachDialogs.tsx`)
- Browser mode: not built to WebAssembly (runtime code); browser mode uses
  `app/ui/src/mock/dapMock.ts`, the same simulated C program.

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-debug`](forge-debug.md),
  [`forge-plugin-api`](forge-plugin-api.md), [`forge-runtools`](forge-runtools.md)
- External: `base64`, `serde`, `serde_json`, `thiserror`, `tokio`, `tracing`

## Tests

12 source files, about 6200 lines; 24 unit tests and 10 integration tests in `tests/`
(`fake_adapter.rs`): launch, breakpoints, stack, threads, variables, stepping, logpoints,
conditions and hit counts, function and exception breakpoints, watchpoints, memory read and
write, disassembly, instruction stepping, goto, set value, attach to a process and to a DAP
server, adapters that listen or connect back, `runInTerminal`, child sessions, core dumps and
restart, all against the fake adapter. No real debugger is needed; an ignored smoke test runs
GDB's DAP server (`gdb -i dap`, GDB 14+) with a C compiler when both are installed.

```sh
cargo test -p forge-dap
cargo test -p forge-dap -- --ignored   # GDB smoke test
```
