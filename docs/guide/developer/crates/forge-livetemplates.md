---
title: forge-livetemplates
description: "Live templates: template sets, contexts, expansion, Surround With and Unwrap/Remove (pure, WebAssembly-friendly)."
order: 27
tags: [crate, rust]
---

# forge-livetemplates

`crates/forge-livetemplates` — Live templates: template sets, contexts, variables and expansion, Surround With and Unwrap/Remove (pure, WebAssembly-friendly).

## Purpose

Everything behind PHPStorm's live templates, Surround With (`Ctrl+Alt+T`) and Unwrap/Remove
(`Ctrl+Shift+Delete`). The UI only shows popups and turns an expansion into CodeMirror
snippet fields.

- `sets`: the built-in groups (`data/*.toml`: PHP, JavaScript, Lua, Twig, Laravel, Blade,
  Symfony) and `resolve`, which applies the user's settings (`editor.liveTemplates.custom`:
  new and changed templates, removed built-ins; `editor.liveTemplates.disabled`) and marks
  framework groups active only for the project's frameworks.
- `context`: the "Applicable in" tree (`PHP_STATEMENT`, `PHP_CLASS_MEMBER`, `JS_EXPRESSION`,
  `HTML`, `BLADE`, …) and `analyze`, which finds the contexts at a position from small lexers
  (PHP with inline HTML, heredocs and attributes; JavaScript with template literals and
  regular expressions; Lua with long strings) plus the class, function, namespace and the
  variables used before the caret.
- `template` and `expr`: `$NAME$` / `$END$` / `$SELECTION$` / `$$` parsing and the variable
  expressions (`className()`, `camelCase(NAME)`, `date("yyyy-MM-dd")`,
  `suggestVariableName(ARRAY)`, …). The clock, clipboard and user name come from the caller.
- `expand`: expansion by abbreviation (`expand`), the Insert Live Template list
  (`applicable`), Surround With (`surround_items`, `surround_apply`). Results are UTF-16
  offsets: the text to insert, one field per variable occurrence (same `index` = edited
  together) and the final caret. Template indentation is converted to the file's indent unit
  and a surrounded selection is re-indented.
- `surround`: the code surrounders per language (PHP, JavaScript, Lua, HTML, Blade, Twig).
- `unwrap`: the statements around the caret (`if`/`elseif`/`else`, loops, `do … while`,
  `try`/`catch`/`finally`, braces; Lua keyword blocks) and the edits that unwrap or remove
  them.

## Main public items

```rust
pub fn catalog(settings: &TemplateSettings) -> Catalog { … }
pub fn resolve(settings: &TemplateSettings) -> Vec<Group> { … }
pub fn analyze(text: &str, at: usize, language_id: Option<&str>, path: &str) -> CaretInfo { … }
pub fn expand(req: &ExpandRequest) -> Option<Expansion> { … }
pub fn applicable(req: &ApplicableRequest) -> ApplicableResult { … }
pub fn surround_items(req: &SurroundRequest) -> Vec<SurroundItem> { … }
pub fn surround_apply(req: &SurroundRequest) -> Option<Expansion> { … }
pub fn unwrap_options(req: &UnwrapRequest) -> Vec<UnwrapOption> { … }
pub fn variable_names(text: &str) -> Vec<String> { … }
```

## Used by

- IPC: `live_templates_catalog`, `live_templates_applicable`, `live_templates_expand`,
  `live_templates_surround_items`, `live_templates_surround`, `live_templates_unwrap`,
  `live_templates_variables`, `live_templates_user` (`app/src-tauri/src/commands/live_templates.rs`)
- UI: `app/ui/src/features/liveTemplates` (expand key, completion, popups, settings page).
  The expand key calls `live_templates_expand` in-process through `forge-wasm` in both modes,
  because the key handler must decide at once whether it expands.
- Browser mode: built into `forge-wasm` (`livetemplates.rs`)

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`, `toml`

## Tests

Unit tests for the lexers, contexts, expressions (case conversions, dates), the template sets
and user changes, expansion (contexts, word boundaries, framework sets, indent units,
expressions), Surround With and Unwrap/Remove in PHP, JavaScript and Lua.

```sh
cargo test -p forge-livetemplates
```
