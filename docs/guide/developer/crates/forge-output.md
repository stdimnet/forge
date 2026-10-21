---
title: forge-output
description: "Forge process output: ANSI styling, console lines, file links, TeamCity test trees and terminal OSC sequences (pure, also built to WebAssembly)."
order: 170
tags: [crate, rust]
---

# forge-output

`crates/forge-output` — Forge process output: ANSI styling, console lines, file links, TeamCity test trees and terminal OSC sequences (pure, also built to WebAssembly).

## Purpose

Process output for the Run tool window and the terminal, computed in Rust so the UI only
renders it.

- `ansi`: ANSI escape sequences (SGR colors and styles) into styled segments.
- `console`: segments into console lines with `\r` progress lines, as deltas the UI
  applies to its line list.
- `links`: clickable file references (`app/User.php:42`, stack traces) and URLs.
- `teamcity`: TeamCity service messages (`phpunit --teamcity`, Pest, Jest) into a test
  tree that is sent to the UI as node updates.
- `run`: everything above combined for one run.
- `osc`: terminal OSC sequences (progress, notifications, working directory) and OSC 8
  link targets.

The crate is pure (no I/O) so `forge-wasm` can run it in browser mode.

## Modules

| Module | Summary |
| --- | --- |
| `ansi.rs` | ANSI escape handling: SGR colors and styles become styled segments, other control sequences (cursor movement, OSC titles) are dropped. Colors are CSS values, either a theme variable (`var(--ansi-red)`) or `rgb(…)`. |
| `console.rs` | Console lines built from styled segments, sent to the UI as deltas. |
| `links.rs` | File references in console and terminal output (`app/User.php:42`, `/abs/path/File.php on line 7`, `File.php(12)`, `#0 /path/x.php(31): …`) and web links, so the UI can make them clickable. |
| `osc.rs` | Payloads of the OSC sequences the terminal understands beyond xterm.js: OSC 9;4 progress (normal, error, indeterminate, paused, remove), OSC 9 and OSC 777 notifications, OSC 7 and ConEmu 9;9 working directories, OSC 133 command boundaries, and which OSC 8 hyperlinks may open (web and mail links, `file://` links into the project). The terminal calls it synchronously through `forge-wasm` from xterm.js's OSC handlers. |
| `run.rs` | The output of one run: console lines for every stream, and for test runners the test tree built from TeamCity service messages on stdout (those lines are not shown in the console). Output while a test runs is attributed to that test. |
| `teamcity.rs` | TeamCity service messages (`phpunit --teamcity`, Pest, Jest reporters) turned into the test tree of the Run tool window. The tree lives here; the UI receives changed nodes. |
| `utf16.rs` | Offsets for the UI: JavaScript strings and CodeMirror count UTF-16 code units, Rust counts bytes. |

## Main public items

```rust
pub use ansi::{AnsiParser, Segment, Style};
pub use console::{ConsoleDelta, ConsoleLine, ConsoleModel, Span};
pub use links::{…};
pub use osc::{link_target, parse_osc, LinkTarget, OscAction, Progress, ProgressState};
pub use run::{OutputEvent, RunInput, RunOutput, RunOutputOptions};
pub use teamcity::{…};
pub use utf16::{byte_to_utf16, utf16_to_byte};
```

## Used by

- IPC: `output_file_links`, and the `console`/`tests` events of `run://<runId>`
- UI: `app/ui/src/features/run`, `app/ui/src/features/terminal`
- Browser mode: built into `forge-wasm`; the terminal uses `osc` through `forge-wasm` in the desktop app too

## Dependencies

- Workspace crates: none
- External: `regex-lite`, `serde`, `serde_json`

## Tests

8 source files, about 2559 lines; 30 unit tests.

```sh
cargo test -p forge-output
```
