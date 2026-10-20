---
title: phpforge-orm-doctrine
description: "PHPForge: Doctrine ORM support (entity mapping, repositories, DQL parsing and completion)."
order: 320
tags: [crate, rust, pack]
---

# phpforge-orm-doctrine

`packs/phpforge/orm-doctrine` — PHPForge: Doctrine ORM support (entity mapping, repositories, DQL parsing and completion).

## Purpose

PHPForge Doctrine plugin: ORM mapping metadata and DQL support.

- `mapping` reads entity mappings from PHP 8 attributes (`#[ORM\Column]`), legacy
  docblock annotations (`@ORM\Column`, parsed by the lang-php docblock parser), XML
  (`*.orm.xml`) and YAML (`*.orm.yml`) mapping files, and finds entity repositories.
- `dql` (from the pure `phpforge-dql` crate) tokenizes and parses DQL for highlighting, completion (entities, aliases,
  fields, associations, keywords and functions) and diagnostics.
- `php` finds DQL inside PHP: `createQuery('…')` (including concatenated strings and
  `Entity::class`) and QueryBuilder calls (`->where('o.status = :s')`), with the aliases
  the builder chain declares.

## Modules

| Module | Summary |
| --- | --- |
| `mapping.rs` | Entity mapping metadata from attributes, annotations, XML and YAML. |
| `php.rs` | DQL inside PHP code: `createQuery()` arguments and QueryBuilder fragments. |

## Main public items

```rust
pub use phpforge_dql::dql;
pub use phpforge_dql::model::{…};
pub struct Config { … }
pub fn is_doctrine_project(root: &Path) -> bool { … }
pub fn scan(root: &Path, config: &Config) -> Snapshot { … }
```

## Used by

- IPC: `doctrine_entities`, `doctrine_complete`, `doctrine_goto`, `doctrine_diagnostics`, `doctrine_highlight`, `doctrine_gutter` (`app/src-tauri/src/commands/doctrine.rs`)
- UI: `app/ui/src/features/doctrine`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`phpforge-dql`](phpforge-dql.md), [`phpforge-lang-php`](phpforge-lang-php.md)
- External: `quick-xml`, `regex`, `serde`, `serde_json`, `tree-sitter`, `walkdir`, `yaml-rust2`

## Tests

3 source files, about 1583 lines; 15 unit tests.

```sh
cargo test -p phpforge-orm-doctrine
```
