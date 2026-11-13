---
title: forge-dbmodel
description: "Forge database explorer model (pure, also built to WebAssembly): the object tree of a data source, schema filter patterns, virtual foreign keys and columns, DDL data sources parsed with sqlparser, full-text search queries, Quick Documentation, the Create/Modify Table scripts and Generate DDL."
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
- `design`: the Create Table and Modify Table dialogs. `TableSpec` (columns with their
  `original` name, primary and unique keys, foreign keys, indexes with a method, checks, table
  options) and `TableSpec::from_table`; `create_statements` / `create_table` per dialect
  (identity clauses, SQLite's inline `INTEGER PRIMARY KEY AUTOINCREMENT`, ClickHouse
  `Nullable(…)` and engine clauses, comments inline or as `COMMENT ON` / `MS_Description`,
  `IF NOT EXISTS` or SQL Server's `IF OBJECT_ID(…) IS NULL`); `alter_table`: a rename-aware diff
  of two specs (columns matched by original name, else a removed and an added column at the same
  position with the same type are a rename; keys and indexes compared under renamed columns)
  producing per-dialect statements in a safe order and `DesignWarning`s for destructive
  changes, with the SQLite table rebuild where `ALTER TABLE` cannot do it; `drop_statements`
  with cascade (or the referencing foreign keys dropped first), `rename_table`,
  `rename_column`; `dialect_info`: types for completion, table options and supported features.
- `script`: Generate DDL — `generate_ddl(model, objects, options)` in dependency order (types,
  sequences, tables topologically sorted by foreign keys with cycle edges added by `ALTER TABLE`
  afterwards, views sorted by the views they read, routines, indexes, triggers), with `DROP …
  IF EXISTS` in reverse order, `IF NOT EXISTS` / `OR REPLACE` / `CREATE OR ALTER` and plain or
  qualified names; `schema_objects` for the SQL Generator's list. The caller formats the script
  with `forge-sqlcode`.

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
| `design/mod.rs` | `TableSpec`, `ColumnSpec`, `KeySpec`, `IndexSpec`, `TableOptions`, `CreateOptions`, `create_statements`, `create_table`, `drop_statements` (`DropRequest`), `rename_table`, `rename_column`, `supports_if_not_exists`. |
| `design/alter.rs` | `alter_table` → `AlterPlan { statements, warnings }`, `match_columns`. |
| `design/types.rs` | `dialect_info` → `DesignDialect` (types, `OptionField`s, features, FK actions, index methods). |
| `script.rs` | `DdlObject`, `DdlOptions`, `generate_ddl`, `schema_objects`. |

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
  `db_match_schemas`, `db_search` (`app/src-tauri/src/commands/db.rs`); `db_design_info`,
  `db_design_load`, `db_design_preview`, `db_drop_sql`, `db_rename_sql`, `db_generate_ddl`
  (`app/src-tauri/src/commands/db_objects.rs`)
- [`forge-dbtransfer`](forge-dbtransfer.md) (the new table of an import)
- Browser mode: [`forge-wasm`](forge-wasm.md) `dbmodel_match_schemas`, `dbmodel_parse_ddl`,
  `dbmodel_object_doc`, `dbmodel_apply_virtual`, `dbmodel_search_rows` (`parsers/dbmodel.rs`);
  `dbobjects_design_info`, `dbobjects_design_spec`, `dbobjects_design_preview`,
  `dbobjects_drop_sql`, `dbobjects_rename_sql`, `dbobjects_generate_ddl` (`dbobjects.rs`)

## Dependencies

- Workspace crates: [`forge-sql`](forge-sql.md)
- External: `regex-lite`, `serde`, `serde_json`, `sqlparser` (without default features), `toml`
  (all MIT or Apache-2.0)

## Tests

12 source files; 38 unit tests: DDL scripts and migrations in PostgreSQL, MySQL (with
`DELIMITER`), SQLite and SQL Server, schema filter patterns, virtual objects (TOML round trip,
merge, validation), search queries per dialect and cell matching, generated DDL and Quick
Documentation, CREATE TABLE per dialect, ALTER diffs (renames tracked and by position, dropped
and added columns, type changes, keys following renames, MySQL `CHANGE`, SQL Server defaults and
comments, Oracle `MODIFY`, ClickHouse, the SQLite rebuild, no statements for an unchanged table),
Drop with cascade, Rename, and Generate DDL ordering (foreign keys, views, cycles, drops,
plain names, whole schemas).

```sh
cargo test -p forge-dbmodel
```
