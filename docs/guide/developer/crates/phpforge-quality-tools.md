---
title: phpforge-quality-tools
description: "PHPForge: PHP quality tools (PHPStan, Psalm, PHP CS Fixer, PHP_CodeSniffer, PHP Mess Detector, Laravel Pint, Rector): configurations, command lines, parsing of their JSON reports with path mapping, and unified diffs; pure."
order: 289
tags: [crate, rust, pack, php, quality tools, phpstan, psalm, php-cs-fixer, phpcs, phpmd, pint, rector]
---

# phpforge-quality-tools

`packs/phpforge/quality-tools` — PHPForge: PHP quality tools (PHPStan, Psalm, PHP CS Fixer,
PHP_CodeSniffer, PHP Mess Detector, Laravel Pint, Rector): configurations, command lines,
parsing of their JSON reports with path mapping, and unified diffs.

## Purpose

Everything [PHP Quality Tools](../../user/php-quality-tools.md) computes. The crate is pure
(no processes or files): the app (`app/src-tauri/src/commands/php_quality_tools.rs`) runs the
command lines through the project backend, directly or wrapped by the tool's PHP interpreter
(`phpforge-interpreters`), and hands the output back to the parsers.

## Modules

| Module | Summary |
| --- | --- |
| `model.rs` | `QualityTool` (ids, labels, `vendor/bin` candidates, phpcbf as phpcs's fixer, the configuration files each tool finds, which tools read unsaved text), `ToolConfig` from the `php.tools.<id>.*` settings (inspection, interpreter, path, phpcbf path, configuration file, level/ruleset, options, timeout, `SeverityChoice`), `split_args` (shell-like option splitting). |
| `command.rs` | `check_args(cfg, config, input)` for `Input::Files`, `Input::Stdin` (phpcs `--stdin-path=<file> -`, PHP CS Fixer `-`) and `Input::TempCopy` (PHPStan `--tmp-file … --instead-of …`); `fix_args` and `FixMode` (phpcbf: stdin → stdout, PHP CS Fixer / Rector: dry-run diff, Pint: in place); `in_place_args` (Actions on Save); `parse_version`. |
| `parse.rs` | `parse_report(tool, stdout, ctx, severity)`: PHPStan `--error-format=json`, Psalm `--output-format=json`, phpcs `--report=json`, PHPMD `json`, PHP CS Fixer / Pint `--format=json` (with or without diffs) and Rector `--output-format=json` as `Problem`s, `FileFix`es (diff and rules) and tool errors. Severities follow the tools (PHPMD priorities, Psalm info as weak warnings, fixers' hunks as weak warnings) unless the configuration overrides them. |
| `problem.rs` | `Problem`, `Severity`, `PathContext` (reported paths relative to the tool's working directory, mapped back from the interpreter with `forge_runtargets::Mapper`, stdin and temporary copy aliases, made relative to the project root). |
| `diff.rs` | Unified diff hunks: `parse`, `apply` (hunks searched near their place, CRLF kept, `ApplyError` when the text changed), `Hunk::changed_lines`. |
| `lib.rs` | Re-exports, `CheckTrigger` and `tools_to_run` (which tools check unsaved text). |

## Main public items

```rust
pub fn check_args(cfg: &ToolConfig, config: Option<&str>, input: &Input) -> ToolArgs;
pub fn fix_args(cfg: &ToolConfig, config: Option<&str>, file: &str, text: &str) -> ToolArgs;
pub fn parse_report(tool: QualityTool, stdout: &str, ctx: &PathContext, severity: SeverityChoice) -> Result<Report, String>;
pub fn tools_to_run(configs: &[ToolConfig], trigger: CheckTrigger, temp_copies: bool) -> Vec<QualityTool>;
pub mod diff; // parse, apply
```

## Used by

- IPC: `quality_tools_info`, `quality_tools_validate`, `quality_tools_check`,
  `quality_tools_inspect`, `quality_tools_fix` (`app/src-tauri/src/commands/php_quality_tools.rs`)
  and the PHP tools of `save_actions_run_tools`
- UI: `app/ui/src/features/phpQualityTools/`, `app/ui/src/ipc/qualityTools.ts`

## Dependencies

- Workspace crates: `forge-runtargets` (path mappings)
- External: `serde`, `serde_json`

## Tests

Unit tests per module and `tests/reports.rs` with recorded reports of every tool in
`tests/fixtures/` (PHPStan from a Docker interpreter, phpcs on stdin, Rector with an error).
The app's tests run the commands against fake tool scripts.

```sh
cargo test -p phpforge-quality-tools
```
