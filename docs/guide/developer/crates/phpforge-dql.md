---
title: phpforge-dql
description: "PHPForge: Doctrine DQL tokenizer, parser, highlighting, completion and diagnostics, and DQL sites in PHP source (pure, also built to WebAssembly)."
order: 290
tags: [crate, rust, pack]
---

# phpforge-dql

`packs/phpforge/dql` — PHPForge: Doctrine DQL tokenizer, parser, highlighting, completion and diagnostics, and DQL sites in PHP source (pure, also built to WebAssembly).

## Purpose

Doctrine DQL for PHPForge, without I/O so it also runs as WebAssembly in browser mode.

- `model`: entity mapping metadata (filled by `phpforge-orm-doctrine`).
- `dql`: tokenizer, parser, semantic highlighting, completion and diagnostics.
- `site`: DQL inside PHP strings (`createQuery()`, QueryBuilder calls) and the
  source-level functions over a `site::SiteFinder`; `lexical` is a finder without a
  PHP parser, `editor` the same functions with UTF-16 offsets.

## Modules

| Module | Summary |
| --- | --- |
| `dql/lexer.rs` | DQL tokenizer. Path expressions (`o.customer.email`) are single identifier tokens; `super::highlight` splits them into alias and field parts. |
| `dql/mod.rs` | DQL: parsing (identification variables and their entities), semantic highlighting, completion and diagnostics against the project's entity mappings. |
| `editor.rs` | The site functions with editor offsets (UTF-16 code units, as JavaScript strings and CodeMirror count), as the IPC commands and the browser mock expose them. |
| `lexical.rs` | A `SiteFinder` without a PHP parser, for browser mode (WebAssembly cannot run the tree-sitter grammar): string literals are found lexically, the call around them by walking back to its `(`, and QueryBuilder aliases are read from the whole file. |
| `model.rs` | Entity mapping metadata as the DQL functions need it. `phpforge-orm-doctrine` fills it from PHP attributes, annotations, XML and YAML; browser mode passes it in as JSON. |
| `site.rs` | DQL inside PHP code: a `Site` is one DQL text assembled from the string literals of a call argument (`createQuery('…')`, QueryBuilder fragments like `->andWhere('o.status = :s')`) with a mapping back to source offsets. |

## Used by

- IPC: `doctrine_complete`, `doctrine_goto`, `doctrine_diagnostics`, `doctrine_highlight` (through phpforge-orm-doctrine)
- UI: `app/ui/src/features/doctrine/editor.ts`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `regex-lite`, `serde`, `serde_json`

## Tests

7 source files, about 2329 lines; 14 unit tests.

```sh
cargo test -p phpforge-dql
```
