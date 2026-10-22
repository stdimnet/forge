---
title: forge-runtools
description: "Run Anything, compound and before-launch plans, External Tools and their macros (pure, also built to WebAssembly)."
order: 222
tags: [crate, rust]
---

# forge-runtools

`crates/forge-runtools` — Run Anything, compound and before-launch plans, External Tools and their macros (pure, also built to WebAssembly).

## Purpose

Pure logic behind Run Anything, compound run configurations, "Before launch" tasks and
External Tools. No I/O, so `forge-wasm` builds it for browser mode; the desktop app
(`app/src-tauri/src/commands/runtools.rs`) collects the inputs (npm and Composer scripts,
PHP files, settings) through the project backend and starts the processes with `run_start`.

## Modules

| Module | Summary |
| --- | --- |
| `args.rs` | Splitting and quoting of argument strings (shared with `forge-process`, which re-exports it). |
| `macros.rs` | PHPStorm's macros (`$FilePath$`, `$ProjectFileDir$`, `$LineNumber$`, `$SelectedText$`, `$ClipboardContent$`, `$Prompt$`, `$PhpExecutable$`, …): the list with descriptions and previews for "Insert Macro…", and expansion with an `Expander` that hands out `$Prompt$` answers in order. Unknown `$Names$` stay as they are; macros without a value expand to nothing and are reported. |
| `external.rs` | External Tools: the model, its settings object (`tools.externalTools`, string values), validation (name, program, output filters), the command of a run (arguments split first, then expanded) and grouping for menus; `action_id` is PHPStorm's `Tool_<group>_<name>`. |
| `launch.rs` | Launch plans: `plan(configurations, target)` returns the configurations to start (a compound's members, nested compounds expanded, duplicates once) and each one's ordered before-launch tasks, with "Run Another Configuration" flattened; missing configurations and cycles are errors. npm, Composer, Artisan and shell tasks come with the temporary configuration that runs them. |
| `anything.rs` | Run Anything: `suggest` groups and ranks (with `forge-fuzzy`) the history, run configurations, npm and Composer scripts, PHP files, plugin commands and the typed command; `?` returns help. `resolve` turns a command into a temporary configuration (`php file.php` → PHP Script, `php artisan …` → Artisan, `composer …`, `npm`/`pnpm`/`yarn`/`bun` → npm, else Shell Script); `push_history` keeps the newest 50 entries. |

## Main public items

```rust
pub struct Configuration { … }   // { name, type, options, beforeLaunch, … }
pub struct Task { … }            // { type, value }
pub mod macros { pub struct MacroContext { … }; pub fn list(..); pub fn expand(..); pub struct Expander<'a> { … } }
pub mod external { pub struct ExternalTool { … }; pub fn command(..); pub fn validate(..); pub fn groups(..) }
pub mod launch { pub fn plan(..) -> Result<LaunchPlan, String>; pub fn task_label(..); pub fn task_run(..) }
pub mod anything { pub fn suggest(..) -> SuggestResponse; pub fn resolve(..); pub fn push_history(..); pub fn composer_scripts(..) }
```

## Used by

- IPC: `run_anything_suggest`, `run_anything_resolve`, `run_anything_history`,
  `run_anything_history_add`, `run_launch_plan`, `external_tool_macros`,
  `external_tool_command`, `external_tool_validate`
- UI: `app/ui/src/features/runAnything`, `features/externalTools`, `features/run` (launch)
- Crates: `forge-process` (argument splitting)
- Browser mode: built into `forge-wasm` (`crates/forge-wasm/src/runtools.rs`)

## Dependencies

- Workspace crates: `forge-fuzzy`, `forge-output` (output filter compilation)
- External: `serde`, `serde_json`

## Tests

6 source files; unit tests for macros, tools, plans, suggestions and command resolution.

```sh
cargo test -p forge-runtools
```
