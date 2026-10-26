---
title: forge-output
description: "Forge process output: ANSI styling, console lines, file links, the test event protocol with its parsers (TeamCity, JUnit, TAP, Go, libtest, Jest/Vitest, pytest, tox) and test trees, and terminal OSC sequences (pure, also built to WebAssembly)."
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
- `filters`: External Tools output filters (`$FILE_PATH$:$LINE$`) as file links.
- `links`: clickable file references (`app/User.php:42`, stack traces) and URLs.
- `teamcity`: TeamCity service messages (`phpunit --teamcity`, Pest, Jest) into a test
  tree that is sent to the UI as node updates.
- `junit`: JUnit XML reports (Codeception, Behat, PHPSpec, pytest) as service messages for
  the same test tree.
- `testevents`: the test event protocol (`TestEvent`: count, suite started / finished, test
  started / failed with expected, actual and a failure location / ignored / output /
  finished with a duration) every format is turned into, and the `LineParser` trait.
- `formats`: parsers for TAP, Go `test2json`, libtest JSON, Jest / Vitest JSON and pytest
  report logs, and `document_events` (JUnit XML, JSON, JSON lines sniffed).
- `run`: everything above combined for one run.
- `osc`: terminal OSC sequences (progress, notifications, working directory) and OSC 8
  link targets.
- `stacktrace`: Analyze External Stack Trace: frames of PHP, Laravel/Symfony, Xdebug,
  JavaScript/Node and Lua traces linked to project files, with server and container paths
  mapped (`PathMapping`, `compose_mappings`, longest existing tail).

The crate is pure (no I/O) so `forge-wasm` can run it in browser mode.

## Modules

| Module | Summary |
| --- | --- |
| `ansi.rs` | ANSI escape handling: SGR colors and styles become styled segments, other control sequences (cursor movement, OSC titles) are dropped. Colors are CSS values, either a theme variable (`var(--ansi-red)`) or `rgb(…)`. |
| `console.rs` | Console lines built from styled segments, sent to the UI as deltas. |
| `filters.rs` | External Tools output filters: a regular expression with `$FILE_PATH$`, `$LINE$` and `$COLUMN$` (`$FILE_PATH$:$LINE$`); `RunOutputOptions.outputFilters` links their matches before the built-in patterns. |
| `testevents.rs` | The test event protocol: `TestEvent` (serde, `event` tag) and `TestEvent::from_service_message`; `LineParser` (`line(&str) -> ParsedLine { events, console, attributed }`, `finish()`) for formats streamed on stdout; `TeamCityParser`. |
| `formats/tap.rs` | TAP 12–14: `ok` / `not ok`, `# SKIP` / `# TODO`, YAML diagnostics (`message`, `expected`/`actual`, `at`, `duration_ms`), busted's comments (`# file @ line`, `Failure message`, `Passed in` / `Expected`), `# Subtest:` nesting (node `--test`, node-tap), plans. |
| `formats/gotest.rs` | Go `test2json`: packages as suites, tests and subtests, output per test, the first `file_test.go:12:` line as message and failure location, `got … want …` values. |
| `formats/libtest.rs` | libtest JSON (`cargo test -- --format json`): a suite per test binary (`Running …` lines), module paths as locations, panic locations and `left` / `right` values. |
| `formats/jest.rs` | Jest / Vitest JSON reports: a suite per file, `describe` blocks from `ancestorTitles`, statuses, `Expected:` / `Received:` values, the failing stack frame, files that failed to run. |
| `formats/tox.rs` | tox runs: every environment is a test (tox 4 and tox 3 action lines, results `OK` / `FAIL` / `SKIP` and the summary), its lines the test's output. |
| `formats/pytest.rs` | pytest report logs (`--report-log`, JSON lines per phase) and the helpers of the `Pytest` JUnit flavor (`module_file`, `assert_values`). |
| `formats/mod.rs` | `line_parser(TestReport)` and `document_events(text, flavor)` (sniffs JUnit XML, Jest JSON and JSON lines). |
| `junit.rs` | JUnit XML reports turned into TeamCity service messages: suites, a suite per class for Codeception's flat suites, failures and errors (with expected/actual from PHPUnit diffs), skipped and the `status` of Behat and PHPSpec (`pending`, `undefined`, `broken`), durations, and locations by `JUnitFlavor` (`file`/`line`; Behat features; PHPSpec spec files and example methods derived from the class). |
| `links.rs` | File references in console and terminal output (`app/User.php:42`, `/abs/path/File.php on line 7`, `File.php(12)`, `#0 /path/x.php(31): …`) and web links, so the UI can make them clickable. |
| `osc.rs` | Payloads of the OSC sequences the terminal understands beyond xterm.js: OSC 9;4 progress (normal, error, indeterminate, paused, remove), OSC 9 and OSC 777 notifications, OSC 7 and ConEmu 9;9 working directories, OSC 133 command boundaries, and which OSC 8 hyperlinks may open (web and mail links, `file://` links into the project). The terminal calls it synchronously through `forge-wasm` from xterm.js's OSC handlers. |
| `stacktrace.rs` | Analyze External Stack Trace: `analyze(text, ctx, exists)` turns a pasted trace into console lines whose frames link to project files (message in red, library frames dimmed); `resolve` maps a path with `TraceContext.mappings`, the project root or the longest trailing part that exists; `compose_mappings` reads Docker Compose bind mounts; `normalize` splits one-line logs with escaped line breaks. |
| `run.rs` | The output of one run: console lines for every stream, and for test runners the test tree built from TeamCity service messages on stdout (those lines are not shown in the console). Output while a test runs is attributed to that test. `TestReport` says where results come from and in which format: streamed lines (TeamCity, TAP, `test2json`, libtest JSON, pytest report logs), a document on stdout (JUnit, Jest JSON; collected and parsed at the exit) or report files handed in as `RunInput::Report` before the exit. `RunOutputOptions.locationRules` and `defaultTestFile` map reported locations to project files. |
| `teamcity.rs` | TeamCity service messages (`phpunit --teamcity`, Pest, Jest reporters) and the test tree of the Run tool window, built from `TestEvent`s (`TestTree::apply_event`; `apply` takes service messages) with location rules (`LocationRule`) and a default file. The tree lives here; the UI receives changed nodes. `TestTree::export` / `import` store a finished tree as `StoredNode`s for the Test History. |
| `utf16.rs` | Offsets for the UI: JavaScript strings and CodeMirror count UTF-16 code units, Rust counts bytes. |

## Main public items

```rust
pub use ansi::{AnsiParser, Segment, Style};
pub use console::{ConsoleDelta, ConsoleLine, ConsoleModel, Span};
pub use links::{…};
pub use osc::{link_target, parse_osc, LinkTarget, OscAction, Progress, ProgressState};
pub use junit::{junit_messages, JUnitFlavor};
pub use run::{OutputEvent, RunInput, RunOutput, RunOutputOptions, TestReport};
pub use testevents::{LineParser, ParsedLine, TestEvent};
pub mod formats { pub fn line_parser(..); pub fn document_events(..); pub fn jest_events(..); … }
pub use teamcity::{…};
pub use utf16::{byte_to_utf16, utf16_to_byte};
```

## Used by

- Crates: `forge-testing` (test framework commands name a `TestReport`), `forge-process`
- IPC: `output_file_links`, the `console`/`tests` events of `run://<runId>` and
  `stacktrace_analyze` (`app/src-tauri/src/commands/stacktrace.rs`)
- UI: `app/ui/src/features/run`, `app/ui/src/features/terminal`, `app/ui/src/features/stacktrace`
- Browser mode: built into `forge-wasm`; the terminal uses `osc` through `forge-wasm` in the desktop app too

## Dependencies

- Workspace crates: none
- External: `regex-lite`, `serde`, `serde_json`

## Tests

Unit tests per module, with recorded outputs of every format in `crates/forge-output/fixtures/`
(busted and node TAP, `go test -json`, libtest JSON, Jest JSON, pytest report log and JUnit).

```sh
cargo test -p forge-output
```
