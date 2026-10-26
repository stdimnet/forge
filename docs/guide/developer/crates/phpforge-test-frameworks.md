---
title: phpforge-test-frameworks
description: "PHPForge: PHP test frameworks (PHPUnit, Pest, Codeception, Behat, PHPSpec): settings and detection, run configuration command lines, test discovery for gutter icons, Gherkin and Behat step definitions, Rerun Failed Tests; pure, also built to WebAssembly."
order: 286
tags: [crate, rust, pack, tests, phpunit, pest, codeception, behat, phpspec, gherkin]
---

# phpforge-test-frameworks

`packs/phpforge/test-frameworks` — PHPForge: PHP test frameworks (PHPUnit, Pest, Codeception,
Behat, PHPSpec): settings and detection, run configuration command lines, test discovery for
gutter icons, Gherkin and Behat step definitions, Rerun Failed Tests.

## Purpose

Everything [PHP test frameworks](../../user/test-frameworks.md) computes. The crate is pure
(no processes, files or network) and builds to WebAssembly for browser mode; the app runs the
tests through the project backend and the PHP interpreter, and the results are parsed by
[`forge-output`](forge-output.md) (TeamCity messages and JUnit reports). The frameworks
register on the shared test runner as one provider (`PhpTestFrameworks`, see
[`forge-testing`](forge-testing.md)), so discovery, Rerun Failed Tests and Run with Coverage
go through the same registry as every other pack's frameworks; the discovery types
(`TestFile`, `TestTarget`, `TargetKind`, `FailedTest`) are `forge-testing`'s.

## Modules

| Module | Summary |
| --- | --- |
| `frameworks.rs` | `Framework` (ids, labels, Composer packages, `vendor/bin` scripts, configuration files), `FrameworkConfig` (Settings \| PHP \| Test Frameworks: interpreter, Composer autoloader or path, configuration and bootstrap files) and its codec for the `php.testFrameworks` objects setting; `detect` (composer.json, `vendor/bin`, configuration files, `tests/Pest.php`), `effective` (configured first, then detected frameworks without an entry) and `resolve` (the entry of an interpreter, else Local). |
| `command.rs` | `build`: the arguments after `php` of Pest (`--teamcity`, `--filter` with the description and Pest's method name, dataset rows), Codeception (suite, file, `file:method`, groups, environments, `--xml` into the report folder), Behat (`--format=progress --out=std --format=junit --out=<dir>`, `file:line`, profile) and PHPSpec (`run --format=junit`, `file:line`, bootstrap) configurations, with the `TestReport` and `JUnitFlavor` of their results; `phpunit_defaults` fills a PHPUnit configuration's executable, configuration and bootstrap from the settings. |
| `discover.rs` | `discover(path, text)`: the tests of a file with their temporary configurations and result keys — PHPUnit classes and methods, Pest `test()` / `it()` / `describe()` calls (nested describes named `` `a` → it b `` like Pest) and inline dataset rows, Codeception Cest/Cept files and methods and `Codeception\Test\Unit` classes, Behat features, scenarios and example rows, PHPSpec specs and `it_*` / `its_*` examples; `target_at` for Run Context Configuration; `pest_evaluable` (Pest's `Str::evaluable`). |
| `gherkin.rs` | `parse`: features, background, scenarios and outlines with tags, steps (`Given`, `When`, `Then`, `And`, `But`, `*`) and example rows (1-based lines, doc strings skipped); `split_step`. |
| `steps.rs` | Behat step definitions from doc comments (`@Given …`) and attributes (`#[Given('…')]`, `#[\Behat\Step\Then("…")]`); `pattern_regex` (delimited regular expressions with flags, turnip `:placeholders`, optional `(s)` and `word/other` alternatives); `step_at` (a step's text, outline placeholders replaced by the first example row) and `step_targets`. |
| `rerun.rs` | `rerun_failed`: PHPUnit / Pest `--filter`, Codeception `-g failed`, Behat `--rerun`, PHPSpec the failed spec file. |
| `provider.rs` | `PhpTestFrameworks`: the `forge_testing::TestFrameworkProvider` of the five types (commands stay with the PHP interpreter path of the backend); `coverage_args` (`--coverage-clover <file>`, `XDEBUG_MODE=coverage`) for PHPUnit and Pest. |

## Used by

- IPC: `test_frameworks_list`, `test_discover`, `test_rerun_failed`,
  `behat_step_definitions`, and `run_command_line` / `run_start` for the `pest`,
  `codeception`, `behat` and `phpspec` configuration types
  (`app/src-tauri/src/commands/test_frameworks.rs`, `commands/run.rs`)
- UI: `app/ui/src/features/run/` (types, gutter, Rerun Failed Tests),
  `app/ui/src/features/testFrameworks/` (settings page, step navigation)
- Browser mode: built into `forge-wasm` (`test_frameworks.rs`)

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md),
  [`forge-testing`](forge-testing.md)
- External: `regex-lite`, `serde`, `serde_json`

## Tests

Unit tests per module, and `tests/recorded_outputs.rs`: recorded Pest TeamCity output and a
Codeception JUnit report (through a container path mapping) built into test trees whose names
match the keys of the discovered tests.
