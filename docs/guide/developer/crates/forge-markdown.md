---
title: forge-markdown
description: "Forge Markdown: a safe AST (no raw HTML) for chat messages and documentation popups, built with pulldown-cmark (pure, also built to WebAssembly)."
order: 140
tags: [crate, rust]
---

# forge-markdown

`crates/forge-markdown` — Forge Markdown: a safe AST (no raw HTML) for chat messages and documentation popups, built with pulldown-cmark (pure, also built to WebAssembly).

## Purpose

Markdown as a safe AST for the UI, which renders it with Solid components (never as
HTML). Raw HTML is never passed through: in chat messages it stays literal text, in
documentation `<br>`/`<p>` become line breaks and every other tag is dropped.

- `parse`: Markdown into `Block`s (`Flavor::Chat` or `Flavor::Docs`).
- `hover`: language server hover Markdown (Intelephense & co.) split into the parts of
  a PHPStorm-style documentation popup.

## Modules

| Module | Summary |
| --- | --- |
| `ast.rs` | The Markdown AST sent to the UI (JSON tagged by `t`). |
| `hover.rs` | Language server hovers (Markdown as Intelephense, phpactor & co. send it) split into the parts of a PHPStorm-style documentation popup: owner FQCN, signature, description and `Params:` / `Returns:` / `Throws:` sections. |
| `parse.rs` | pulldown-cmark events into the `Block` tree. |

## Main public items

```rust
pub use ast::{Block, Inline, ListItem};
pub use hover::{parse_hover, HoverDoc, HoverSection};
pub use parse::{parse, parse_inline, Flavor};
```

## Used by

- IPC: `markdown_parse`, `markdown_hover` (`app/src-tauri/src/commands/markdown.rs`)
- UI: `app/ui/src/features/lsp/MarkdownDoc.tsx`, `app/ui/src/features/ai-chat/Markdown.tsx`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `pulldown-cmark`, `regex-lite`, `serde`, `serde_json`

## Tests

4 source files, about 987 lines; 9 unit tests.

```sh
cargo test -p forge-markdown
```
