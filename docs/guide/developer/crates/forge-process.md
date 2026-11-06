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
| `args` | Splitting and quoting of argument strings as typed into run configuration fields ("Arguments", "Interpreter options", "Test runner options"); re-exported from `forge-runtools`. |
| `config.rs` | Run/Debug configurations. |
| `pty.rs` | Interactive shell sessions on a pseudo terminal, used by the Terminal tool window. `foreground_process()` names the program running instead of the shell (the terminal's foreground process group on Unix), which the project lifecycle asks about before a project closes. |
| `terminal_layout.rs` | The Terminal tool window of each project: `TerminalLayout` (tabs with id, title, renamed, working directory, shell profile; the selected tab), sanitized and stored as workspace state in `<state dir>/forge/terminal-layouts/<project key>.json`, and the optional scrollback per tab, capped at 256 KB, in `<cache dir>/forge/terminal-scrollback/<project key>/`. |
| `runner.rs` | Non-interactive processes for run configurations: piped stdout/stderr streamed as text, stdin for input typed into the Run console, the exit code, and kill of the whole process tree (so `composer run` or `npm run` children stop too). |
| `shell.rs` | Picking the user's default shell. |
| `test_history.rs` | The Test History of the Run tool window: `TestRunRecord` (configuration, start time, summary, counts and the stored tree), saved as compact JSON in `<data dir>/forge/test-history/<project key>/<id>.json`, listed newest first and pruned to the last `MAX_RUNS` (10) per project. |
| `termenv.rs` | The environment a terminal session starts with: `TERM`, `COLORTERM`, `TERM_PROGRAM=Forge`, `TERM_PROGRAM_VERSION` and `FORGE_TERMINAL=1`, a UTF-8 `LANG` when the inherited locale is not UTF-8, and removal of variables that describe another terminal or multiplexer (kitty, tmux, iTerm2, WezTerm, Ghostty, …). |
| `utf8.rs` |  |

## Main public items

```rust
pub use forge_output as output;
pub use args::{quote_arg, split_args};
pub use config::{…};
pub use pty::{PtyEvent, PtyOptions, PtySession, PtySink};
pub use runner::{CommandLine, OutputStream, RunEvent, RunSink, RunningProcess};
pub use shell::default_shell;
pub use termenv::{terminal_env, utf8_locale_fallback};
pub use utf8::Utf8Decoder;
```

## Used by

- IPC: `terminal_open`, `terminal_write`, `terminal_resize`, `terminal_close`, `terminal_running`, `terminal_layout_load`, `terminal_layout_save`, `run_configurations_load`, `run_configurations_save`, `run_command_line`, `run_start`, `run_stop`, `run_write`, `run_dispose`, `test_history_list`, `test_history_open`
- UI: `app/ui/src/features/run`, `app/ui/src/features/terminal`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-output`](forge-output.md)
- External: `portable-pty`, `serde`, `serde_json`, `toml`, `tracing`

## Tests

8 source files, about 2040 lines; 26 unit tests.

```sh
cargo test -p forge-process
```
