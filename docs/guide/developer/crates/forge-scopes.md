---
title: forge-scopes
description: "Scopes (PHPStorm's scope language, predefined and named scopes) and file colors (pure, WebAssembly-friendly)."
order: 232
tags: [crate, rust, scopes]
---

# forge-scopes

`crates/forge-scopes` — Scopes (PHPStorm's scope language, predefined and named scopes) and file colors (pure, WebAssembly-friendly).

## Purpose

Scopes and file colors, like PHPStorm's Settings | Appearance & Behavior | Scopes and File
Colors (see the [user guide](../../user/scopes-and-file-colors.md)).

- `pattern`: the scope language: `file:` / `src:` / `test:` / `lib:` path patterns (`*`, `?`,
  `//` for any folders, a pattern without `/` matches file names), `file[module]:`, `&&`,
  `||`, `!`, parentheses, with parse errors that carry an offset. `Expr::matches` for files,
  `Expr::matches_dir` for folders as a whole.
- `Predefined` scopes (All Places, Project Files, Project Production Files, Project Test
  Files, Open Files, Changed Files, Current File, Non-Project Files), `NamedScope` and
  `ScopeSet` (shared and local scopes of the settings: entries for choosers, name lookup,
  problems).
- `ScopeFilter` (a `ScopeSpec` plus its `ScopeContext`: test roots, open, changed and current
  files) is what searches carry to the backend; `Scope` is it compiled.
- `context`: test detection (`composer_test_roots` from `autoload-dev`, conventional folders
  and file names) and library paths.
- `edit`: the Include / Include Recursively / Exclude / Exclude Recursively buttons.
- `tree`: the Scopes page's preview tree with per-folder counts.
- `colors`: file colors per scope with PHPStorm's precedence (local rows first, first match
  wins) and the theme palette.

## Main public items

```rust
pub fn parse(text: &str) -> Result<Option<Expr>, ParseError>
pub enum Predefined { AllPlaces, ProjectFiles, ProjectProductionFiles, ProjectTestFiles, OpenFiles, ChangedFiles, CurrentFile, NonProjectFiles }
pub struct ScopeContext { test_roots, open_files, changed_files, current_file }
pub enum ScopeSpec { Predefined(Predefined), Pattern(String) }
pub struct ScopeFilter { scope: ScopeSpec, context: ScopeContext }
pub struct Scope { … } // new(&ScopeFilter), contains(path), contains_dir(dir)
pub struct ScopeSet { shared, local } // resolve, entries, problems
pub mod edit;   // EditRequest, EditOp, apply, file_pattern
pub mod tree;   // build -> ScopeTree
pub mod colors; // ColorsRequest, FileColor, resolve, PALETTE
```

## Used by

- `forge-find` (`FindRequest.scope`), `forge-codeintel` (`UsageScope` custom patterns),
  `forge-ssr` (`SsrRequest.scope`)
- IPC: `scopes_files` (`Backend::list_files`, agent call `ListFiles`) and `scopes_test_roots`
  (`app/src-tauri/src/commands/scopes.rs`)
- UI: `app/ui/src/features/scopes` calls it synchronously through `forge-wasm` (`scopes_*`,
  `file_colors_resolve`) in the desktop app and in browser mode

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

6 source files; unit tests for patterns, operators, sources, folders, errors, predefined and
named scopes, Include / Exclude, the preview tree and file colors.

```sh
cargo test -p forge-scopes
```
