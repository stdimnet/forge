---
title: webforge-js-testing
description: "WebForge: the Node.js test runners (Jest, Vitest, Mocha, Playwright Test, Cypress, node --test) on the shared test runner: discovery of describe/it/test calls, command lines, Rerun Failed Tests and debug programs — pure, also built to WebAssembly."
order: 374
tags: [crate, rust, pack, tests, jest, vitest, mocha, playwright, cypress]
---

# webforge-js-testing

`packs/webforge/js-testing` — WebForge's Node.js test runners on the shared test runner
([forge-testing](forge-testing.md)); pure, also built to WebAssembly.

## Purpose

WebForge's manifest declares each runner as `[[contributions.testFrameworks]]` (detection,
test file globs, arguments per scope, report format, rerun pattern, coverage, extra form
fields). `JsTestRunner` wraps such a spec and replaces the plain spec provider of the same id
in the `TestFrameworks` registry (`register`):

- `discovery`: a small JavaScript scanner (strings, template literals, comments and regular
  expressions are skipped) finds `describe` / `context` / `suite` and `it` / `test` /
  `specify` calls with `.only`, `.skip`, `.each(…)(…)`, `test.describe` and other modifiers;
  suites end where their call closes, so full names do not depend on indentation.
- `runner`: `Runner` (`jest`, `vitest`, `mocha`, `playwright`, `cypress`, `node-test`) with
  its package, CLI script and debug arguments. `build` runs the CLI script with Node.js
  (options `executable`, `nodeOptions`, `package`, `configFile`), sets
  `PLAYWRIGHT_JSON_OUTPUT_NAME` for Playwright's report file, wraps Mocha in c8 or nyc for
  coverage (`coverage_wrap`, also used for Run with Coverage of Node.js configurations),
  passes the coverage provider of Vitest (`--coverage.provider`) and Jest
  (`--coverageProvider`) and puts `node --test`'s options before its files. `claims` decides between
  runners that share file globs by the file's imports. `rerun` adds the failed tests' files
  to the spec's name pattern (Cypress reruns files). `debug_program` gives the
  `runtimeExecutable`, `runtimeArgs`, script and arguments js-debug launches.

## Main public items

```rust
pub enum Runner { Jest, Vitest, Mocha, Playwright, Cypress, NodeTest }
pub struct JsTestRunner { pub runner: Runner, pub spec: TestFrameworkSpec }   // TestFrameworkProvider
pub fn register(registry: &mut TestFrameworks, specs: impl IntoIterator<Item = TestFrameworkSpec>);
pub fn discovery::discover(text: &str) -> Vec<Found>;
pub struct NodeProgram { pub runtime: String, pub runtime_args: Vec<String>, pub program: String, pub args: Vec<String>, … }
```

## Used by

- IPC: the test registry of `run_start`, `run_command_line`, `test_discover` and
  `test_rerun_failed` (`app/src-tauri/src/commands/run.rs`, `commands/testing.rs`);
  `js_debug_request` through [webforge-js-debug](webforge-js-debug.md)
- Browser mode: built into `forge-wasm` (`testing.rs`)

## Dependencies

- Workspace crates: [`forge-runtools`](forge-runtools.md), [`forge-testing`](forge-testing.md)
- External: `serde`

## Tests

Unit tests run against WebForge's real manifest (`app/src-tauri/plugins/webforge.toml`):
detection from `package.json` and config files, discovery and the choice between runners,
command lines per runner and scope, coverage arguments, Rerun Failed Tests and debug programs.

```sh
cargo test -p webforge-js-testing
```
