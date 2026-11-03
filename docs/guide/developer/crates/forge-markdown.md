---
title: forge-markdown
description: "Forge Markdown: a safe AST (no raw HTML) for chat messages, documentation popups and the Markdown preview, built with pulldown-cmark (pure, also built to WebAssembly)."
order: 150
tags: [crate, rust]
---

# forge-markdown

`crates/forge-markdown` — Forge Markdown: a safe AST (no raw HTML) for chat messages, documentation popups and the Markdown preview, built with pulldown-cmark (pure, also built to WebAssembly).

## Purpose

Markdown as a safe AST for the UI, which renders it with Solid components (never as
HTML). Raw HTML is never passed through: in chat messages it stays literal text, in
documentation `<br>`/`<p>` become line breaks and every other tag is dropped.

- `parse`: Markdown into `Block`s (`Flavor::Chat`, `Flavor::Docs` or `Flavor::Preview`).
- `parse_document`: a Markdown file for the preview: GitHub-flavored blocks (tables, task
  lists, strikethrough, footnotes, autolinks, alerts, heading anchors, images) with the source
  lines of every top-level block (synchronized scrolling), the front matter and the footnotes.
  Raw HTML is sanitized into the AST (`<img>`, `<a href>`, `<br>`, `<b>`, `<i>`, `<s>`); comments
  and every other tag are dropped.
- `hover`: language server hover Markdown (php-lsp & co.) split into the parts of
  a PHPStorm-style documentation popup.

## Modules

| Module | Summary |
| --- | --- |
| `ast.rs` | The Markdown AST sent to the UI (JSON tagged by `t`). |
| `doc_comment.rs` | Documentation comments for the editor's rendered view and Reader mode: `doc_comments(text, language)` finds `/** … */` (PHPDoc, JSDoc / TSDoc) and `---` (LuaDoc / LuaCATS) comments, `parse_doc_comment` turns one into a `HoverDoc`: the description as Markdown (inline `{@link url text}` / `{@see X}` as links and code), the tags as `Params:` / `Returns:` / `Throws:` / `Properties:` / `See also:` … sections, `@deprecated` as the note and `@example` as a code block. |
| `document.rs` | The Markdown preview of a file: GitHub-flavored blocks with the source lines of every top-level block, the front matter and the footnotes; `anchor_line` for links into a file. |
| `hover.rs` | Language server hovers (Markdown as php-lsp & co. send it) split into the parts of a PHPStorm-style documentation popup: owner FQCN, signature, description and `Params:` / `Returns:` / `Throws:` sections. |
| `parse.rs` | pulldown-cmark events into the `Block` tree. |

## Main public items

```rust
pub use ast::{Block, Inline, ListItem};
pub use doc_comment::{doc_comments, doc_style, parse_doc_comment, DocComment, DocStyle};
pub use document::{anchor_line, parse_document, Document, Footnote, FrontMatter, FrontMatterEntry, LineRange};
pub use hover::{parse_hover, HoverDoc, HoverSection};
pub use parse::{parse, parse_inline, Flavor};
```

## Used by

- IPC: `markdown_parse`, `markdown_hover`, `markdown_document`, `markdown_anchor_line` (`app/src-tauri/src/commands/markdown.rs`)
- UI: `app/ui/src/features/lsp/MarkdownDoc.tsx`, `app/ui/src/features/ai-chat/Markdown.tsx`, `app/ui/src/features/markdown/MarkdownPreview.tsx`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `pulldown-cmark`, `regex-lite`, `serde`, `serde_json`

## Tests

5 source files; 17 unit tests.

```sh
cargo test -p forge-markdown
```
