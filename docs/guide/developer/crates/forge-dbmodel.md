---
title: forge-dbmodel
description: "Forge database explorer model (pure, also built to WebAssembly): the object tree of a data source, schema filter patterns, virtual foreign keys and columns, DDL data sources parsed with sqlparser, full-text search queries and Quick Documentation."
order: 61
tags: [crate, rust]
---

# forge-dbmodel

`crates/forge-dbmodel` — the Database Explorer model of Forge: pure code shared by `forge-db`
(desktop) and the browser-mode database backend (WebAssembly).

## Purpose

- `model`: the object tree of a data source — `DbModel` (dialect, connected database and the
  others, every schema with its filter result, introspected schemas, roles, warnings),
  `SchemaInfo` (tables, views, materialized views, routines with arguments, sequences, types),
  `TableInfo` (columns with generated/virtual expressions, keys, foreign keys, indexes, checks,
  triggers, view query). `forge-db` caches `DbModel` per data source in the user cache dir.
- `filter`: the schema filter of a data source's Schemas tab: names, `*`/`?` globs,
  `:current`, `/regex/` (`/regex/i`) and `!pattern` exclusions; no patterns means the current
  schema.
- `virtuals`: virtual foreign keys and columns kept in the project
  (`.forge/database/virtual.toml`), validated and merged into a schema.
- `ddl`: DDL data sources — SQL files parsed with `sqlparser` in the file's dialect and applied
  in order (`CREATE`, `ALTER TABLE`, `CREATE INDEX`, `DROP`, `COMMENT ON`); MySQL `DELIMITER`;
  routines, triggers and sequences the parser does not take are picked up by their header;
  everything else is a warning with file and line.
- `search`: full-text search — the per-dialect `SELECT` for one table (`ILIKE`, `LIKE … ESCAPE`,
  `REGEXP_LIKE`, `~*`, `positionCaseInsensitiveUTF8`, `match`; the text only as a quoted
  literal), searchable columns, and `CellMatcher` for highlighting matching cells.
- `generate`, `doc`: DDL generated from the model (tables with checks, views, sequences, types,
  routines, triggers) and the Quick Documentation of a tree node (`ObjectRef` → `ObjectDoc`).

### Decisions

- **sqlparser** (Apache-2.0) parses DDL: it knows the dialects Forge supports and is pure Rust,
  so the same parser runs in the desktop app and in browser mode. Statements are split by
  `forge-sql` first, so one bad statement never hides the rest of a file.
- The model types moved here from `forge-db::introspect` (which re-exports them) so DDL parsing,
  documentation and search can be pure and built to WebAssembly.

## Modules

| Module | Summary |
| --- | --- |
| `model.rs` | The object tree: `DbModel`, `SchemaName` (with `visible`), `SchemaInfo`, `TableInfo`, `ColumnInfo`, `ForeignKey`, `IndexInfo`, `CheckInfo`, `TriggerInfo`, `RoutineInfo`/`RoutineArg`, `SequenceInfo`, `TypeInfo`, `RoleInfo`, `ModelWarning`; incremental `merge_table`. |
| `filter.rs` | `SchemaFilter`: parse and apply schema filter patterns. |
| `virtuals.rs` | `VirtualObjects` (`foreignKey`, `column`): TOML file format, per data source replace, validation, merge into a schema. |
| `ddl.rs` | `parse(dialect, default_schema, files)` → `ParsedDdl { schemas, warnings }`; `trigger_head` for trigger timing and events. |
| `search.rs` | `SearchOptions`, `table_query`, `searchable_columns`, `is_text_type`, `CellMatcher`. |
| `generate.rs` | `table`, `sequence`, `user_type`, `routine`, `trigger` DDL. |
| `doc.rs` | `document(dialect, schema, object)` and `document_role`. |

## Main public items

```rust
pub use doc::{document, ObjectDoc, ObjectRef};
pub use filter::{apply_patterns, SchemaFilter};
pub use model::*;
pub use search::{CellMatcher, SearchMode, SearchOptions};
pub use virtuals::{VirtualColumn, VirtualForeignKey, VirtualObjects, VIRTUAL_FILE};
```

## Used by

- [`forge-db`](forge-db.md) (introspection, the explorer service, `ddl::generate`); re-exported
  as `forge_db::model`.
- IPC: `db_model`, `db_object_doc`, `db_virtual_objects`, `db_virtual_objects_save`,
  `db_match_schemas`, `db_search` (`app/src-tauri/src/commands/db.rs`)
- Browser mode: [`forge-wasm`](forge-wasm.md) `dbmodel_match_schemas`, `dbmodel_parse_ddl`,
  `dbmodel_object_doc`, `dbmodel_apply_virtual`, `dbmodel_search_rows` (`parsers/dbmodel.rs`)

## Dependencies

- Workspace crates: [`forge-sql`](forge-sql.md)
- External: `regex-lite`, `serde`, `serde_json`, `sqlparser` (without default features), `toml`
  (all MIT or Apache-2.0)

## Tests

8 source files; 21 unit tests: DDL scripts and migrations in PostgreSQL, MySQL (with
`DELIMITER`), SQLite and SQL Server, schema filter patterns, virtual objects (TOML round trip,
merge, validation), search queries per dialect and cell matching, generated DDL and Quick
Documentation.

```sh
cargo test -p forge-dbmodel
```
