---
title: forge-emmet
description: "Emmet: abbreviation parser and expander for HTML, JSX and CSS, Surround with Emmet, edit points, Update/Remove Tag (pure, WebAssembly-friendly)."
order: 77
tags: [crate, rust]
---

# forge-emmet

`crates/forge-emmet` — Emmet: abbreviation parser and expander for HTML, JSX and CSS, Surround with Emmet, edit points, Update/Remove Tag (pure, WebAssembly-friendly).

## Purpose

Everything behind PHPStorm's Emmet support except the key binding and the popups: where an
abbreviation ends and starts, which syntax applies at the caret, the parsed abbreviation, the
expanded text with its tab stops, and the edits of the tag actions. The UI
(`app/ui/src/features/emmet`) binds the expand key and applies the result as a CodeMirror
snippet. All offsets the UI sends and receives are UTF-16.

- `abbr`: the parser. Child `>`, sibling `+`, climb `^`, multiplication `*N` (a bare `*`
  repeats once per wrapped line), groups `( )`, `#id`, `.class`, `[a=b c="d" e. f={expr}]`,
  `{text}`, numbering `$`, `$$`, `$@-`, `$@3`, self-closing `/`, names with `:` and `!` and
  the expanding snippets with `+` (`ul+`). The result is an arena tree (`Tree`).
- `snippets`: Emmet's HTML snippets (`a` → `a[href]`, `input:c`, `link:css`, `!`), the inline,
  void and known element names and the implicit tag of an element without a name.
- `markup`: resolves snippets, implicit tags, numbering, lorem and repeats, inserts wrapped
  text (`$#` or the innermost last element, URL recognition for `a`), and writes HTML or JSX
  formatted like Emmet (block elements on their own lines, runs of fewer than three inline
  elements kept on one line, `html` children not indented) with tab stops for empty attribute
  values and empty elements.
- `css`: property abbreviations (a table of Emmet's CSS snippets, exact and fuzzy property
  names, keywords by prefix or initials, numbers with units and unit aliases, unitless
  properties, colors, `!important`, `+` for several declarations, at-rules).
- `lorem`: deterministic placeholder text.
- `context`: the syntax family of a file type, template code where markup does not apply
  (`{{ }}`, `{% %}`, `@php`), CSS declaration positions, the JSX positions, and the extraction
  of the abbreviation before the caret (balanced brackets, a `>` that closes a tag ends it).
- `html`: a tolerant tag scanner: the state at a position (text, tag, comment, PHP, script,
  style), tags, element pairs and the attributes of an opening tag.
- `actions`: edit points, Update Tag with Emmet and Remove Tag as byte edits.

## Main public items

```rust
pub fn expand(req: &ExpandRequest) -> Option<Expansion> { … }
pub fn wrap(req: &WrapRequest) -> Result<Expansion, String> { … }
pub fn edit_point(req: &EditPointRequest) -> Option<usize> { … }
pub fn update_tag(req: &TagRequest) -> Result<Vec<Edit>, String> { … }
pub fn remove_tag(req: &TagRequest) -> Result<Vec<Edit>, String> { … }
pub fn markup::expand(abbr: &str, opts: &MarkupOptions, wrap: Option<&[String]>) -> Result<Output, String> { … }
pub fn css::expand(abbr: &str, opts: &CssOptions) -> Result<Output, String> { … }
```

`Expansion` (`{ from, to, text, fields: { from, to, index }[], end, abbreviation, syntax }`)
has the shape of a live template expansion, so the UI inserts both with the same snippet code.

## Used by

- IPC: `emmet_expand`, `emmet_wrap`, `emmet_edit_point`, `emmet_update_tag`,
  `emmet_remove_tag` (`app/src-tauri/src/commands/emmet.rs`).
- UI: `app/ui/src/features/emmet` (expand key, abbreviation preview, Surround with Emmet,
  Update Tag, Remove Tag, edit points, Settings | Editor | Emmet). The expand key and the
  preview call `emmet_expand` in-process through `forge-wasm` in both modes.
- Browser mode: built into `forge-wasm` (`emmet.rs`).

## Dependencies

- Workspace crates: none
- External: `serde` (`serde_json` for tests)

## Tests

Unit tests for the parser (operators, climbing, groups, attributes, text, numbering, errors),
every snippet, formatting, implicit tags, numbering variants, lorem, the HTML5 document, JSX,
tab stops, wrapping (lines, `$#`, URLs), limits, CSS numbers, units, keywords, colors and
rules, the HTML scanner, extraction and contexts per language, edit points, Update Tag, Remove
Tag and UTF-16 offsets.
