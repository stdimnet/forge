---
title: forge-ssr
description: "Structural Search and Replace: templates with $variables$ matched on tree-sitter syntax trees (PHP, JavaScript, TypeScript)."
order: 238
tags: [crate, rust, search, tree-sitter]
---

# forge-ssr

`crates/forge-ssr` — Structural Search and Replace: templates with `$variables$` matched on tree-sitter syntax trees (PHP, JavaScript, TypeScript).

## Purpose

PHPStorm's Edit | Find | Search Structurally… and Replace Structurally… (see the
[user guide](../../user/structural-search.md)).

- `template`: the template becomes a syntax tree. Every `$name$` is replaced by a placeholder
  the grammar accepts at that place (in PHP an identifier after `->`, `::`, `new`, in types
  and declarations, a variable elsewhere; `$$name$` only matches variables); the template may
  be an expression, statements or a class member (parsed inside a wrapper class).
- `matcher` (private): node kinds, named leaves, keywords and operators must be equal;
  comments and punctuation are skipped; a placeholder matches any node, or a run of sibling
  nodes within the variable's count (greedy, with backtracking); a variable used twice must
  match the same text. Declarations in the template without return types, attributes or
  modifiers match declarations with them.
- `types` (private): the Type modifier's types from the tree (literals, `new X`, `$this`,
  typed parameters, `$x = new X` in the same function).
- `Compiled::search` / `replace`, `search_files` with a `forge_scopes::ScopeFilter`, unsaved
  buffers and a limit, `check` for the dialog, `library::builtin` templates.

The feature `engine` (default) holds everything that needs tree-sitter (C code). Without it
the crate keeps the request types, `variables` and the template library, which
`forge-wasm` builds for browser mode.

## Main public items

```rust
pub enum FileType { Php, JavaScript, TypeScript }
pub struct Constraint { name, min_count, max_count, text, invert_text, type_, invert_type }
pub struct SsrRequest { template, file_type, constraints, match_case, replacement, scope, buffers, limit }
pub struct SsrResult { files: Vec<SsrFile>, match_count, searched_files, truncated, error }
pub struct Compiled { … } // new, wants, search, replace
pub fn search_files<'a>(req: &SsrRequest, files: impl IntoIterator<Item = (&'a str, &'a str)>) -> SsrResult
pub fn check(req: &SsrRequest) -> TemplateCheck
pub fn variables(template: &str) -> Vec<String>
pub mod library; // SavedTemplate, builtin()
```

## Used by

- `forge-backend`: `Backend::structural_search` (local: `ssr::search_workspace`; remote: agent
  call `StructuralSearch`) and `ssr::replace_all` (writes through the backend, undo like
  Replace in Files)
- IPC: `ssr_templates`, `ssr_check`, `ssr_search`, `ssr_replace`
  (`app/src-tauri/src/commands/ssr.rs`)
- UI: `app/ui/src/features/ssr`
- Browser mode: `forge-wasm` without the engine (`ssr_templates`, `ssr_variables`)

## Dependencies

- Workspace crates: `forge-scopes`
- External: `regex`, `serde`, `tree-sitter`, `tree-sitter-php`, `tree-sitter-javascript`,
  `tree-sitter-typescript`

## Tests

Unit tests for placeholders and template parsing, every built-in template compiling, and
fixture tests (`tests/fixtures/OrderService.php`, `cart.ts`) for method calls, counts, text and
type modifiers, statements, sequences, class members, repeated variables, positions,
replacement with indentation, TypeScript/TSX/JavaScript, scopes, buffers and limits.

```sh
cargo test -p forge-ssr
```
