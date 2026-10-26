---
title: forge-testing
description: "The shared test runner and coverage model of every pack: test framework contributions (detection, command lines, gutter discovery, Rerun Failed Tests, location rules), the provider registry, and coverage reports (LCOV, Cobertura, Clover, JaCoCo, Go coverprofile) with summaries and line marks; pure, also built to WebAssembly."
order: 241
tags: [crate, rust, tests, coverage, test runner, contribution point]
---

# forge-testing

`crates/forge-testing` — the shared test runner and coverage model of every pack (pure, also
built to WebAssembly).

## Purpose

What [Testing](../../user/testing.md) and [Code coverage](../../user/coverage.md) compute
besides the test results themselves (those are [`forge-output`](forge-output.md)'s test event
protocol and parsers). Packs register test frameworks as manifest data or as code; the backend
asks one registry for gutter discovery, command lines and Rerun Failed Tests, and reads
coverage reports into one model.

## Modules

| Module | Summary |
| --- | --- |
| `targets.rs` | `TestFile` (the tests of a file with their temporary configurations and result keys), `TestTarget`, `TargetKind` (`class`, `method`, `file`, `describe`, `test`, `dataset`, `feature`, `scenario`, `example`, `suite`), `FailedTest`. |
| `provider.rs` | The contribution point: `TestFrameworkProvider` (`id`, `types`, `accepts`, `discover`, `rerun_failed`, `command`, `supports_coverage`), `CommandContext` (report folder, coverage file, settings), `TestCommand` (program, arguments, environment, `TestReport`, `JUnitFlavor`, location rules, default file, coverage request) and the `TestFrameworks` registry (`register`, `register_specs`, `for_type`, `discover`, `discover_in_project` (the frameworks the project uses first), `rerun_failed`, `command`, `info`). |
| `spec.rs` | `TestFrameworkSpec` (with `fields`: extra run configuration form fields), the manifest contribution `[[contributions.testFrameworks]]`: detection (globs, dependency files), test file globs, line-pattern discovery nested by indentation with full names, per-scope arguments with placeholders (`${file}`, `${test}`, `${testPattern}`, `${pattern}`, `${reportDir}`, `${coverageFile}`), escaping (regex, Lua patterns), rerun by pattern, location rules and coverage; `SpecProvider`; `glob_regex`, `expand`. |
| `coverage/mod.rs` | `CoverageReport` (files with `LineHits`: hits, branches, partial), `LineStatus` (covered, partial, uncovered), `merge`, `resolve` (reported paths → project paths), `sniff` / `parse_any`, `Counter` ("85% (17/20)"), `summary` (the Coverage tool window's project, compacted directories and files) and `FileCoverage::marks` (gutter marks with hover texts). |
| `coverage/lcov.rs` | LCOV tracefiles (`SF`, `DA`, `BRDA`). |
| `coverage/xml.rs` | Cobertura (`<source>` folders, class lines without the method duplicates), Clover (`stmt`, `method`, `cond` with true/false counts) and JaCoCo (covered/missed instructions and branches) in one streaming pass. |
| `coverage/gocover.rs` | Go coverage profiles: blocks over lines, partly covered lines. |

## Main public items

```rust
pub trait TestFrameworkProvider: Send + Sync { fn id(&self) -> &str; fn types(&self) -> Vec<String>; fn accepts(&self, path: &str) -> bool; fn discover(..) -> Option<TestFile>; fn rerun_failed(..) -> Option<Configuration>; fn command(..) -> Option<Result<TestCommand, String>>; fn supports_coverage(&self, type_id: &str) -> bool; }
pub struct TestFrameworks { … }        // register, register_specs, for_type, discover, rerun_failed, command
pub struct TestFrameworkSpec { … }     // re-exported by forge-plugin-api as TestFrameworkContribution
pub struct CoverageReport { pub files: Vec<FileCoverage> }
pub fn coverage::summary(report: &CoverageReport, project: &str) -> Vec<CoverageNode>;
```

## Used by

- IPC: `test_discover`, `test_rerun_failed`, `test_frameworks_detect`, `coverage_suites`,
  `coverage_view`, `coverage_file`, `coverage_remove`, `coverage_import`, and the command
  lines of manifest frameworks in `run_start` / `run_command_line`
  (`app/src-tauri/src/commands/testing.rs`, `commands/run.rs`)
- Crates: [`forge-plugin-api`](forge-plugin-api.md) (the `testFrameworks` contribution),
  [`phpforge-test-frameworks`](phpforge-test-frameworks.md) (the PHP provider),
  [`pyforge-testing`](pyforge-testing.md) (pytest, unittest and tox),
  [`webforge-js-testing`](webforge-js-testing.md) (the Node.js runners wrap their specs)
- UI: `app/ui/src/features/run` (manifest framework types, gutter), `app/ui/src/features/coverage`
- Browser mode: built into `forge-wasm` (`testing.rs`)

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md)
- External: `quick-xml`, `regex-lite`, `serde`, `serde_json`

## Tests

Unit tests per module with the busted manifest table and recorded coverage reports (LCOV,
Cobertura, Clover, JaCoCo, Go profile) in `crates/forge-testing/fixtures/`.

```sh
cargo test -p forge-testing
```
