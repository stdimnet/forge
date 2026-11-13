---
title: forge-save-actions
description: "Actions on Save: which actions run for a saved file, reformatting only changed lines and running external tools (ESLint, Prettier, Stylelint, Biome, Rector, PHP CS Fixer, phpcbf, Laravel Pint) through the project's backend."
order: 225
tags: [crate, rust]
---

# forge-save-actions

`crates/forge-save-actions` — Actions on Save: which actions run for a saved file,
reformatting only changed lines and running external tools (Biome, ESLint, Stylelint,
Prettier, Rector, PHP CS Fixer, phpcbf, Laravel Pint) through the project's backend.

## Purpose

The logic behind Settings | Tools | [Actions on Save](../../user/actions-on-save.md):

- `plan(path, trigger, settings)` decides which actions run for one save: Reformat code
  (whole file or changed lines, limited to file types), Optimize imports, Rearrange code and
  Run code cleanup (PHP files only) and the external tools. On autosave nothing runs unless
  "Also run on autosave" is on, and the external tools never do. The web tools run for files
  matching their "Run for files" globs (`web.eslint.files`, `web.stylelint.files`,
  `web.biome.files`, `actionsOnSave.prettier.files`) unless their mode is `disabled`;
  whether Biome takes over ESLint and Prettier depends on the project's files and is decided
  where the tools run.
- `restrict_to_changed_lines(base, original, formatted)` keeps only the hunks of a
  reformatting that touch lines changed since `base` (the committed text; `None` = the whole
  file is new).
- `tools` (feature `backend`, on by default) finds a tool in the project (`vendor/bin`,
  `node_modules/.bin`, `.bat` / `.cmd` too) or at its custom path and runs it on the saved
  file through a `forge_backend::Backend`, so remote projects work. A failing tool's changes
  are rolled back; ESLint's and Biome's exit code 1 (problems left after fixing) counts as
  success, and Stylelint's 2 and phpcbf's 1 and 2 too. `run_tools_with` passes each command line through a callback first:
  the app gives the PHP tools their Quality Tools arguments and PHP interpreter
  ([phpforge-quality-tools](phpforge-quality-tools.md)), and the web tools the package found
  for the file run with the Node.js interpreter
  ([webforge-quality-tools](webforge-quality-tools.md)).

The PHP actions themselves are in `phpforge-lang-php` (`imports`, `rearrange`, `cleanup`).
The UI applies the combined result as one edit (one undo step).

## Modules

| Module | Summary |
| --- | --- |
| `lib.rs` | Settings keys, `ActionsOnSave` (settings), `Trigger`, `Tool`, `SavePlan`, `plan`, `file_type_choices`. |
| `changed.rs` | "Changed lines": a line diff (LCS) between the original and the reformatted text, applied hunk by hunk. |
| `tools.rs` | Locating and running Biome, ESLint, Stylelint, Prettier, Rector, PHP CS Fixer, phpcbf and Laravel Pint; `run_tools`, `run_tools_with`, `detect_tools`. |

## Main public items

```rust
pub fn plan(path: &str, trigger: Trigger, settings: &ActionsOnSave) -> SavePlan;
pub fn restrict_to_changed_lines(base: Option<&str>, original: &str, formatted: &str) -> String;
pub fn file_type_choices() -> Vec<FileTypeChoice>;
pub mod tools; // run_tools, detect_tools, locate (feature "backend")
```

## Used by

- IPC: `save_actions_plan`, `save_actions_changed_lines`, `save_actions_php`,
  `save_actions_run_tools`, `save_actions_info` (`app/src-tauri/src/commands/save_actions.rs`)
- UI: `app/ui/src/features/actionsOnSave/`, `saveFile` in `app/ui/src/state/editor.ts`
- Browser mode: `plan` and `restrict_to_changed_lines` are built into `forge-wasm`
  (without the `backend` feature)

## Dependencies

- Workspace crates: `forge-filetypes`, `forge-text`; with `backend`: `forge-backend`,
  `forge-core`, `forge-process`
- External: `serde`, `serde_json`

## Tests

Plan, changed lines and tool runs with fake tool scripts in temporary directories (no real
tools needed).

```sh
cargo test -p forge-save-actions
```
