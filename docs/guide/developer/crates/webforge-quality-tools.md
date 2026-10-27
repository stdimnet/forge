---
title: webforge-quality-tools
description: "WebForge: web code quality tools (ESLint, Prettier, Stylelint, Biome): configurations, automatic detection of packages and configuration files, command lines, and parsing of their JSON reports; pure."
order: 375
tags: [crate, rust, pack, javascript, css, quality tools, eslint, prettier, stylelint, biome]
---

# webforge-quality-tools

`packs/webforge/quality-tools` — WebForge: web code quality tools (ESLint, Prettier,
Stylelint, Biome): configurations, automatic detection of packages and configuration files,
command lines, and parsing of their JSON reports.

## Purpose

Everything [Web Code Quality Tools](../../user/web-quality-tools.md) computes. The crate is
pure (no processes or files): it reads the project through the `ProjectFs` trait, and the app
(`app/src-tauri/src/commands/web_quality_tools.rs`) runs the command lines through the
project backend with the Node.js interpreter and hands the output back to the parsers.

ESLint runs through its CLI like Stylelint and Biome (`--format json --stdin
--stdin-filename`), so all three linters share one path: stdin in, a JSON report out, parsed
here with recorded fixtures.

## Modules

| Module | Summary |
| --- | --- |
| `model.rs` | `WebTool` (ids, labels, npm packages, default bin scripts, configuration file names, package.json keys, default "Run for files" globs, settings keys), `Mode` (Disabled / Automatic / Manual), `ToolConfig` from `web.<id>.*` and the tools' Actions on Save keys, `WebToolsSettings` (on the fly, timeout, Biome priority, Node.js interpreter). |
| `detect.rs` | `ProjectFs`; `nearest_config` (walking up from the file: config files, then the package.json key), `nearest_package` (`node_modules/<package>` walking up), `bin_entry` (the package's `bin` field); `locate(fs, cfg, file)` → `Located` (launcher `node <script>` or an executable, version, configuration file, working directory, ESLint flat config) or `Skip` (disabled, not matched, no config, no package, Biome first); `locate_all` / `applicable` with Biome's priority; `formatter` for Reformat Code (Biome before Prettier). |
| `command.rs` | `Invocation` (node or direct, program, args, stdin, env, cwd); `lint`, `lint_paths` (Inspect Code), `fix` (ESLint `--fix-dry-run`, Stylelint `--fix` on stdin, Biome `lint --write`), `format` (Prettier `--stdin-filepath`, Biome `format`), `fix_in_place` (on save), `version`, `parse_version`. Manual configuration files become `--config` / `--config-path`; `ESLINT_USE_FLAT_CONFIG` follows the configuration found. |
| `parse.rs` | `parse_report(tool, stdout, stderr, ctx, source)`: ESLint `--format json` (messages, fixable, `output` of `--fix-dry-run`, ignored-file warnings skipped), Stylelint `--formatter json` (stdout before 16, stderr since; rule suffix stripped, invalid option warnings as errors), Biome `--reporter=json` (byte spans or line/column positions, markup messages, rule links) as `Problem`s. |
| `problem.rs` | `Problem`, `Severity`, `PathContext` (reported paths relative to the tool's working directory and stdin names mapped to the project file), `line_col` (byte offset → line and character column). |
| `lib.rs` | Re-exports and `exit_ok` (exit codes that still carry a report). |

## Main public items

```rust
pub trait ProjectFs { fn read(&self, path: &str) -> Option<String>; fn list(&self, dir: &str) -> Vec<String>; }
pub fn locate(fs: &dyn ProjectFs, cfg: &ToolConfig, file: &str) -> Result<Located, Skip>;
pub fn applicable(fs: &dyn ProjectFs, settings: &WebToolsSettings, file: &str) -> Vec<Located>;
pub fn formatter(fs: &dyn ProjectFs, settings: &WebToolsSettings, file: &str) -> Option<Located>;
pub fn command::lint(l: &Located, cfg: &ToolConfig, root: &str, file: &str, text: &str) -> Invocation;
pub fn parse_report(tool: WebTool, stdout: &str, stderr: &str, ctx: &PathContext, source: &dyn Fn(&str) -> Option<String>) -> Result<Report, String>;
```

## Used by

- IPC: `web_quality_tools_info`, `web_quality_tools_validate`, `web_quality_tools_check`,
  `web_quality_tools_inspect`, `web_quality_tools_fix`, `web_quality_tools_format`
  (`app/src-tauri/src/commands/web_quality_tools.rs`) and the web tools of
  `save_actions_run_tools`
- UI: `app/ui/src/features/webQualityTools/`, `app/ui/src/ipc/webQualityTools.ts`

## Dependencies

- Workspace crates: `forge-filetypes` (globs with `{a,b}`), `forge-runtools` (option splitting)
- External: `serde`, `serde_json`

## Tests

Unit tests per module (an in-memory project for detection) and `tests/reports.rs` with
recorded reports in `tests/fixtures/` (ESLint 9 with and without `--fix-dry-run`, Stylelint 16
on SCSS, Biome 1.9). The app's tests run the commands against fake packages and a fake
`node`.

```sh
cargo test -p webforge-quality-tools
```
