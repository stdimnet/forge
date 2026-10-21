---
title: phpforge-lang-php
description: "PHPForge: PHP language support (tree-sitter index, docblocks, hover, code generation, language server config)."
order: 320
tags: [crate, rust, pack]
---

# phpforge-lang-php

`packs/phpforge/lang-php` — PHPForge: PHP language support (tree-sitter index, docblocks, hover, code generation, language server config).

## Purpose

PHPForge language pack: PHP support built on tree-sitter.

- `parser` indexes a file into a `model::FileIndex` (namespaces, imports, classes,
  members, functions, docblocks and attributes).
- `docblock` parses PHPDoc including Doctrine/Symfony annotations.
- `index::ProjectIndex` indexes a workspace in parallel with an mtime-keyed cache,
  fuzzy symbol search, file structure and lazy vendor resolution.
- `hover` renders index-based hover markdown.
- `generate` implements the "Generate" actions (constructor, accessors, overrides, ...).
- `navigate` Go to Declaration, Find Usages, implementations and gutter markers.
- `stubs` PHP's own symbols from phpstorm-stubs (the "PHP Runtime").
- `lsp` describes how to launch Intelephense / phpactor.

## Modules

| Module | Summary |
| --- | --- |
| `docblock.rs` | PHPDoc parser: summary/description, the standard tags (`@param`, `@return`, `@throws`, `@var`, `@property[-read\|-write]`, `@method`, `@deprecated`, `@see`, `@template`, `@extends`/`@implements`), static-analysis prefixes (`@phpstan-`/`@psalm-`) and Doctrine/Symfony style annotations (`@ORM\Column(type="string")`, `@Route("/x")`). |
| `fuzzy.rs` | Fuzzy matcher for Go to Class / Go to Symbol: subsequence matching with bonuses for word starts, camel humps (`OrCo` -> `OrderController`) and consecutive runs. |
| `generate.rs` | "Generate" actions (Alt+Insert): constructor, getters/setters, `__toString()`, override/implement method stubs and PHPDoc blocks. |
| `hover.rs` | Index-based hover for project symbols: signature + attributes + parsed docblock as markdown. Complements a language server's hover (which covers built-ins). |
| `intentions.rs` | Built-in intentions for Show Context Actions (Alt+Enter) on the index. |
| `index.rs` | Workspace-wide PHP index: parallel indexing, mtime-keyed persistent cache, fuzzy symbol search, ancestor lookup and lazy resolution of vendor classes through Composer's PSR-4 metadata. |
| `lsp.rs` | How to launch a PHP language server: Intelephense (preferred, users bring their own licence key) with phpactor as fallback. Independent of the generic LSP client; the app turns `LspServerConfig` into a running server. |
| `model.rs` | Symbol model produced by the indexer. Everything is serializable so it can be cached on disk and sent over IPC unchanged (camelCase field names for the UI). |
| `navigate.rs` | Code navigation on the index when no language server answers: the symbol under the caret (classes with or without a leading backslash through imports, aliases, group uses and the namespace; functions and constants with PHP's global fallback; members through `$this`, `self`/`static`/`parent`, typed variables, `catch`, `@var`, property and return-type chains; variables), `definition` (target, "on a declaration" or an `ExternalSymbol` for the stubs), `find_usages` (usage kinds, dynamic usages, members through parent and child classes), Go to Implementation / Super Method / Type Declaration and the implemented / overridden `line_markers`. |
| `offsets.rs` | Offset conversion. The index works in UTF-8 byte offsets; the UI (CodeMirror / JS strings) uses UTF-16 code units. Convert at the IPC boundary. |
| `parser.rs` | Tree-sitter based PHP file indexer: turns source into a `FileIndex`. |
| `resolve.rs` | Name resolution through namespaces and use-imports (PHP's rules, simplified to what an index needs: classes and type hints). |
| `service.rs` | Editor-facing PHP queries on a `ProjectIndex`: the same answers wherever the index lives (in the desktop app or in the remote development agent). Offsets crossing this boundary are UTF-16 (JS strings); the index works in bytes, so every query converts. |
| `structure.rs` | File structure for the Structure tool window (Alt+7). |
| `stubs.rs` | `StubIndex`: PHP's own classes, functions and constants from a phpstorm-stubs folder (also `define()` constants); `lookup` answers an `ExternalSymbol` (members through stub ancestors), `definition_in` navigates inside a stub file. |

## Main public items

```rust
pub use index::{IndexConfig, IndexStats, ProjectIndex};
pub use model::*;
pub enum PhpError { … }
```

## Used by

- IPC: `php_index_workspace`, `php_search_symbols`, `php_file_structure`, `php_hover`, `php_generate_targets`, `php_generate`, `php_detect_servers`, `php_lsp_config` (`app/src-tauri/src/commands/php.rs`); `code_goto_declaration`, `code_symbol_info`, `code_find_usages`, `code_navigate`, `code_line_markers`, `code_context_actions`, `library_read` (`commands/codeintel.rs`, `commands/library.rs`) through `forge_backend::PhpBackend`, so remote projects answer from the agent
- UI: `app/ui/src/features/php`, `app/ui/src/features/codeintel`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: `forge-codeintel`
- External: `dirs`, `rayon`, `serde`, `serde_json`, `thiserror`, `tracing`, `tree-sitter`, `tree-sitter-php`, `walkdir`

## Tests

16 source files, about 10000 lines; 80 unit tests.

```sh
cargo test -p phpforge-lang-php
```
