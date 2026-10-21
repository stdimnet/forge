---
title: forge-editing
description: "Editor actions computed in Rust: postfix completion templates, Complete Current Statement and Cyclic Expand Word (pure, WebAssembly-friendly)."
order: 75
tags: [crate, rust]
---

# forge-editing

`crates/forge-editing` — Editor actions computed in Rust: postfix completion templates, Complete Current Statement and Cyclic Expand Word (pure, WebAssembly-friendly).

## Purpose

Editor actions that need a little language knowledge, shared by the desktop backend and the
browser-mode WebAssembly build. Offsets at the API boundary are UTF-16 code units, like
CodeMirror positions.

- `postfix`: PHPStorm's postfix templates for PHP, JavaScript/TypeScript and Lua (`$user.if`,
  `items.forof`, `list.ipairs`). `complete_postfix` finds the expression before `.key` in the
  caret's line (the closest expression for templates like `.not` or `.isset`, the whole
  statement for `.if`, `.return` or `.foreach`) and expands every applicable template with
  the line's indentation, a suggested variable name (`$this->getUsers().foreach` →
  `$user`) and the caret or selection. `templates` lists all templates with a before/after
  example for the settings page.
- `statement`: Complete Current Statement. Closes the brackets the line left open, adds a
  missing `;` (not inside multi-line arrays, calls or object literals, nor before a chained
  `->`/`.` line), opens the block of control statements (`if (…) {`), puts PHP function and
  class braces on the next line (PSR-12) and completes Lua's `then … end`, `do … end`,
  `function … end` and `repeat … until`.
- `hippie`: Cyclic Expand Word. The words that start with the prefix at the caret: above the
  caret first (nearest first), then below, then in the other open files; the backward action
  looks below first.
- `lang`: the language families (`php`, `js`, `lua`) and their Forge file type ids.

## Main public items

```rust
pub enum Lang { Php, Js, Lua }
pub fn templates() -> Vec<TemplateInfo> { … }
pub fn complete_postfix(request: &PostfixRequest) -> Option<PostfixCompletion> { … }
pub fn complete_statement(request: &StatementRequest) -> Option<StatementEdit> { … }
pub fn expand_word(request: &ExpandRequest) -> ExpandResult { … }
```

## Used by

- IPC: `editor_postfix_templates`, `editor_postfix_complete`, `editor_complete_statement`,
  `editor_expand_word` (`app/src-tauri/src/commands/editor.rs`)
- UI: `app/ui/src/features/editing` (completion source, expand key, Settings | Editor |
  General | Postfix Completion, the `EditorCompleteStatement`, `HippieCompletion` and
  `HippieBackwardCompletion` actions)
- Browser mode: built into `forge-wasm` (`editing_*`)

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

5 source files; unit tests for every template family, expression and statement detection,
strings and comments, UTF-16 offsets, statement completion in PHP, JavaScript and Lua, and
word expansion order.

```sh
cargo test -p forge-editing
```
