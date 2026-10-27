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
- `navigate` Go to Declaration, Find Usages (with the Find Usages options), implementations
  and gutter markers.
- `highlight` Highlight Usages in File: the usages in the current file with read / write
  access, the implementing / overriding methods on `implements` / `extends`.
- `hierarchy` Type, Method and Call Hierarchy.
- `stubs` PHP's own symbols from phpstorm-stubs (the "PHP Runtime").
- `lsp` describes how to launch Intelephense / phpactor.

## Modules

| Module | Summary |
| --- | --- |
| `attributes.rs` | JetBrains PhpStorm attributes from the indexed attributes (parameters included): `deprecated` (also PHP 8.4's `#[\Deprecated]`), `render_replacement` (`%parametersList%`, `%parameterN%`, `%class%`, `%name%`), `expected_values` (`values`, `flags`, `valuesFromClass`, `flagsFromClass`), `array_shape` / `object_shape`, `immutable` (write scope), `language`, `has(attrs, "Pure")`, plus PHP literal helpers (`array_elements`, `key_value`, `string_literal`). |
| `callsite.rs` | The call around a position, text based so it works while typing (`call_at`, `scan_to`: open brackets, argument index, the string literal the caret is in), the `Callee` (function candidates, method of a class chain, constructor) with `matches(MetaCallable)`, its declaration (`callee_decl` → `Decl`) and `#[ArrayShape]` resolution (`shape_of`, `subscript_classes`). |
| `cleanup.rs` | Code Cleanup: removes unused imports, unnecessary semicolons (empty statements in statement lists, never a loop's empty body) and a redundant closing tag `?>` at the end of a pure PHP file. Used by Actions on Save. |
| `complete.rs` | Completion from metadata and attributes (`complete`): expected arguments (`expectedArguments()`, `#[ExpectedValues]`, `override()` map keys), expected return values in comparisons, `#[ArrayShape]` keys; values inside a string without quotes. |
| `create.rs` | Create from usage (Alt+Enter on an unresolved name): "Create method / field / constant / enum case / function" as edits (another file's as a `WorkspaceEdit` with LSP positions) with the caret after the edit, parameters and return types inferred from the usage, bodies from the code template "PHP Implemented Method Body"; "Create class / interface / trait" as the `PhpCreateClass` action with the dialog's data. Each fix carries the `ExternalSymbol` the PHP stubs must not declare. |
| `docblock.rs` | PHPDoc parser: summary/description, the standard tags (`@param`, `@return`, `@throws`, `@var`, `@property[-read\|-write]`, `@method`, `@deprecated`, `@see`, `@template`, `@extends`/`@implements`), static-analysis prefixes (`@phpstan-`/`@psalm-`) and Doctrine/Symfony style annotations (`@ORM\Column(type="string")`, `@Route("/x")`). |
| `fuzzy.rs` | Fuzzy matcher for Go to Class / Go to Symbol: subsequence matching with bonuses for word starts, camel humps (`OrCo` -> `OrderController`) and consecutive runs. |
| `generate.rs` | "Generate" actions (Alt+Insert): constructor, getters/setters, `__toString()`, override/implement method stubs and PHPDoc blocks. |
| `highlight.rs` | Highlight Usages in File on the index (when no language server answers `documentHighlight`): the usages of the symbol at the caret in its file, the declaration as a write, and on `implements` / `extends` the class's methods that implement / override the parents' methods. |
| `hover.rs` | Index-based hover for project symbols: signature + attributes + parsed docblock as markdown. Complements a language server's hover (which covers built-ins). |
| `intentions.rs` | Built-in intentions for Show Context Actions (Alt+Enter) on the index: replace qualifier with an import, accessors, "Add 'readonly' modifier", method stubs, PHPDoc and the "Generate..." link. |
| `imports/` | Optimize Imports: removes `use` imports nothing refers to (code, attributes, docblocks and annotations count) and arranges the rest by the code style's Imports tab; group uses are expanded, braced namespaces handled separately. `style.rs`: `ImportStyle` (sorting, grouping, blank lines, Prefer FQN and the PHP Auto Import settings) and the style of the running call (`with_style`, `set_default`). `insert.rs`: `insert_imports`, the one helper every import goes through (Import class, completion, Create from usage, Move, Copy, Pull Members Up). `auto.rs`: Auto Import: unresolved class names with candidates (`ProjectIndex::class_candidates`, project and vendor by file name), `reference_edits` (import, short name or FQN), the "Import class" fix with the chooser, the import popup and the on-the-fly decisions. |
| `hierarchy.rs` | Type, Method and Call Hierarchy on the index (`HierarchyQuery`, answer `PhpHierarchy { graph?, title?, root?, calls }` in `forge_codeintel::hierarchy`'s model): the class at the caret with all its supertypes (parents, interfaces, traits; vendor classes resolved lazily, unknown ones by name) and every project class below it; for a method also the subtypes of its topmost declaration and each type's `MethodPresence`; the callable at the caret, its callers (`find_usages` call sites grouped by the enclosing method, function or file) and callees (calls in its body resolved like Go to Declaration, constructors through `new`, PHP's own functions as library items). |
| `index.rs` | Workspace-wide PHP index: parallel indexing, mtime-keyed persistent cache, fuzzy symbol search, ancestor lookup and lazy resolution of vendor classes through Composer's PSR-4 metadata. |
| `injections.rs` | `language_places`: every `#[Language]` parameter of the indexed functions, methods and constructors, handed to `forge-injections` (`InjectionSettings::places`). |
| `inspections.rs` | Forge's PHP inspections (`inspect`, `fixes_at`): deprecated symbols with "Replace with …", expected values, `#[Immutable]` writes, unused `#[Pure]` results, unreachable statements after `#[NoReturn]` / `exitPoint()` calls, and "Namespace name doesn't match PSR-0/PSR-4 project structure" with "Set namespace to …" (source roots of the index), plus the data-flow inspections. Calls and names resolve through `navigate::resolve_word`. `PhpProblem::code` is the inspection id of [`forge-inspections`](forge-inspections.md) (`PhpDeprecation`, `PhpExpectedValuesShouldBeUsed`, `PhpImmutablePropertyIsChanged`, `PhpExpressionResultUnused`, `PhpUnreachableStatement`, `PhpIllegalPsrClassPath`, …); `to_problem` turns a problem into the framework's `Problem`. |
| `dataflow.rs` | PHP data flow on the tree-sitter tree, per function, method and closure (and the top level): values of local variables through straight-line code and branches (merged at joins, loops forget what they assign), reporting "Variable is always null" (`PhpVariableIsAlwaysNull`), "Undefined variable" (`PhpUndefinedVariable`), "Condition is always true / false" (`PhpConstantCondition`) and "Unused assignment" (`PhpUnusedAssignment`, with "Remove unused assignment" / "Remove assignment, keep the expression"). Functions with `$$name`, `extract()`, `compact()`, `get_defined_vars()`, `eval`, `include` or `goto` are skipped; variables shared by reference are never tracked. |
| `lsp.rs` | How to launch a PHP language server: Intelephense (preferred, users bring their own licence key) with phpactor as fallback. Independent of the generic LSP client; the app turns `LspServerConfig` into a running server. |
| `meta.rs` | `.phpstorm.meta.php` (`is_meta_path`, `parse` → `PhpMeta`): `override()` with `map()` (`@` patterns, `::class` keys), `type()`, `elementType()`; `expectedArguments()`, `expectedReturnValues()`, `registerArgumentsSet()` / `argumentsSet()`, `exitPoint()`; lookups `function_return` / `method_return` (→ `MetaType`), `map_entry`, `map_keys`, `expected_arguments`, `expected_return_values`, `is_exit_point`. `ProjectIndex::meta()` merges the files of the project, vendor packages and include paths (cached with the index, read again on change). |
| `model.rs` | Symbol model produced by the indexer. Everything is serializable so it can be cached on disk and sent over IPC unchanged (camelCase field names for the UI). |
| `navigate.rs` | Code navigation on the index when no language server answers: the symbol under the caret (classes with or without a leading backslash through imports, aliases, group uses and the namespace; functions and constants with PHP's global fallback; members through `$this`, `self`/`static`/`parent`, typed variables, `catch`, `@var`, property and return-type chains; variables), `definition` (target, "on a declaration" or an `ExternalSymbol` for the stubs), `find_usages` / `find_usages_with` (usage kinds, dynamic usages, members through parent and child classes; with the Find Usages options also usages of a class's methods and fields, derived / implementing classes, derived interfaces and overriding methods, comments and strings), Go to Implementation / Super Method / Type Declaration and the implemented / overridden `line_markers`. |
| `rearrange.rs` | Rearrange Code: orders class members as trait uses, enum cases, constants, properties, constructor, methods; comments move with their member, blank lines stay in place. |
| `rename.rs` | Rename (Shift+F6) on the index: PHPStorm's dialog label for a symbol and the name clashes reported as conflicts (`clashes`: the class / function / constant or member with the new name already exists). The edit itself comes from `find_usages` and `forge_codeintel::edit_from_locations`. |
| `offsets.rs` | Offset conversion. The index works in UTF-8 byte offsets; the UI (CodeMirror / JS strings) uses UTF-16 code units. Convert at the IPC boundary. |
| `project.rs` | `PhpProjectSettings` (include paths, `php.sourceRoots`, composer.json path; `effective_roots`), "External Libraries" (`library_roots`, `list_library`, `read_library`, only inside include paths). |
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

- IPC: `php_inspect`, `php_complete`, `php_language_places`, `php_libraries`, `php_library_list`, `php_source_roots` (`commands/php_meta.rs`), `php_index_workspace`, `php_search_symbols`, `php_file_structure`, `php_hover`, `php_generate_targets`, `php_generate`, `php_detect_servers`, `php_lsp_config` (`app/src-tauri/src/commands/php.rs`); `code_goto_declaration`, `code_symbol_info`, `code_find_usages`, `code_highlight_usages`, `code_usage_target`, `code_navigate`, `code_line_markers`, `code_prepare_rename`, `code_rename`, `code_context_actions`, `library_read` (`commands/codeintel.rs`, `commands/library.rs`) through `forge_backend::PhpBackend`, so remote projects answer from the agent
- UI: `app/ui/src/features/php`, `app/ui/src/features/codeintel`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: `forge-codeintel`, `forge-file-templates`, `forge-inspections`, `forge-scopes`, `phpforge-composer`, `phpforge-interpreters`
- External: `dirs`, `rayon`, `serde`, `serde_json`, `thiserror`, `tracing`, `tree-sitter`, `tree-sitter-php`, `walkdir`

## Tests

16 source files, about 10000 lines; 80 unit tests.

```sh
cargo test -p phpforge-lang-php
```
