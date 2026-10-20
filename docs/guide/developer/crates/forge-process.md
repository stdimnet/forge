---
title: forge-process
description: "Forge IDE processes: PTY terminal sessions, run configurations and the process runner."
order: 190
tags: [crate, rust]
---

# forge-process

`crates/forge-process` — Forge IDE processes: PTY terminal sessions, run configurations and the process runner.

## Purpose

Forge processes: everything the IDE starts on behalf of the user.

- `pty`: interactive terminal sessions on a pseudo terminal (Terminal tool window).
- `runner`: plain processes with piped output for run configurations (Run tool window),
  with exit codes and process-tree kill.
- `config`: Run/Debug configurations, their persistence in
  `<project>/.forge/runConfigurations.toml` and the registry of configuration kinds that
  turns a configuration into a command line.
- `output`: the pure output models of the `forge-output` crate (ANSI, console lines,
  file links, TeamCity test trees), re-exported for the run backend.

## Modules

| Module | Summary |
| --- | --- |
| `args.rs` | Splitting and quoting of argument strings as typed into run configuration fields ("Arguments", "Interpreter options", "Test runner options"). |
| `config.rs` | Run/Debug configurations. |
| `pty.rs` | Interactive shell sessions on a pseudo terminal, used by the Terminal tool window. |
| `runner.rs` | Non-interactive processes for run configurations: piped stdout/stderr streamed as text, stdin for input typed into the Run console, the exit code, and kill of the whole process tree (so `composer run` or `npm run` children stop too). |
| `shell.rs` | Picking the user's default shell. |
| `utf8.rs` |  |

## Main public items

```rust
pub use forge_output as output;
pub use args::{quote_arg, split_args};
pub use config::{…};
pub use pty::{PtyEvent, PtyOptions, PtySession, PtySink};
pub use runner::{CommandLine, OutputStream, RunEvent, RunSink, RunningProcess};
pub use shell::default_shell;
pub use utf8::Utf8Decoder;
```

## Used by

- IPC: `terminal_open`, `terminal_write`, `terminal_resize`, `terminal_close`, `run_configurations_load`, `run_configurations_save`, `run_command_line`, `run_start`, `run_stop`, `run_write`, `run_dispose`
- UI: `app/ui/src/features/run`, `app/ui/src/features/terminal`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-output`](forge-output.md)
- External: `portable-pty`, `serde`, `serde_json`, `toml`, `tracing`

## Tests

7 source files, about 1814 lines; 21 unit tests.

```sh
cargo test -p forge-process
```
