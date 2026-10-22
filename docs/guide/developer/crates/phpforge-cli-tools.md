---
title: phpforge-cli-tools
description: "PHPForge: Command Line Tool Support: tool model and detection, Symfony Console `list --format=json` and WP-CLI `cmd-dump` parsing, console input parsing and quoting, completion and the per-tool command cache; pure, also built to WebAssembly."
order: 284
tags: [crate, rust, pack, command line tools, composer, artisan, symfony, wp-cli]
---

# phpforge-cli-tools

`packs/phpforge/cli-tools` — PHPForge: Command Line Tool Support (Composer, Artisan, Symfony
Console, WP-CLI, Drush and custom tools): tool model and detection, list/help JSON parsing,
command line parsing and completion.

## Purpose

Everything the [Command Line Tools Console](../../user/command-line-tools.md) computes. The crate
is pure (no processes, files or network) and builds to WebAssembly for browser mode; the app
runs the tools through the project backend and the PHP interpreter.

## Modules

| Module | Summary |
| --- | --- |
| `model.rs` | `ToolKind` (composer, artisan, symfony, wp-cli, drush, symfony-based, php-script) with PHPStorm's labels, default aliases and paths; `Tool` and its codec for the `commandLineTools.tools` objects setting; `detect` (composer.json, artisan, bin/console, wp-cli.phar, vendor/bin/wp, wp-config.php, vendor/bin/drush) and `effective_tools` (configured first, then detected kinds that are not configured). |
| `definitions.rs` | `CommandDef` / `ArgumentDef` / `OptionDef`; `parse_symfony_list` (Symfony Console's `list --format=json`, shared by Artisan, Symfony, Composer and Drush; argument order kept, PHP's `[]` for empty objects, defaults, shortcuts like `-v\|-vv\|-vvv`, application options marked global, text before the JSON skipped) and `parse_wp_cli_dump` (`wp cli cmd-dump`: subcommands as multi-word commands with their groups, synopsis tokens, descriptions, defaults and `options:` values from `longdesc`). |
| `cmdline.rs` | `tokenize` (POSIX-like quotes and escapes with UTF-16 offsets per word), `split`, `quote` / `join` for display. |
| `complete.rs` | `complete(input, offset, tools, commands)`: aliases, commands (prefix, word, substring and `m:mi`-style abbreviations; WP-CLI words), options (unused ones, command options before global ones), option values (`--format=`, the word after an option that needs a value), arguments (`help <command_name>`, `list <namespace>`, known values), the hint for the expected argument and `CommandHelp` (the command with the argument or option at the cursor). |
| `invoke.rs` | `run_invocation` (PHP script or own program, `--ansi` / `--color` unless given, `COLUMNS=120`), `list_invocation` (`list --format=json`, `cli cmd-dump`), `parse_input` (alias lookup with PHPStorm-like errors). |
| `cache.rs` | `fingerprint` (FNV-1a of kind, path, interpreter and composer.lock), the cache file name per project and alias, and the cache entry format. |

## Used by

- IPC: `cli_tools_list`, `cli_tools_commands`, `cli_tools_complete`, `cli_tools_run`
  (`app/src-tauri/src/commands/command_line_tools.rs`; runs stream `run://<runId>` through
  `commands::run::start_built`, the cache lives in `<cache dir>/forge/command-line-tools`)
- UI: `app/ui/src/features/cliTools/`
- Browser mode: built into `forge-wasm` (`cli_tools.rs`)

## Dependencies

- Workspace crates: phpforge-interpreters (`Program`)
- External: `serde`, `serde_json`

## Tests

7 source files; unit tests per module plus `tests/fixtures.rs` on recorded output
(`tests/fixtures/artisan-list.json`, `composer-list.json`, `symfony-list.json`,
`wp-cli-dump.json`). No PHP is needed; the app's tests run a fake `php` script.

```sh
cargo test -p phpforge-cli-tools
```
