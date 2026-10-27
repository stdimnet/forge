---
title: forge-dbschema
description: "Forge database schema tools (pure, also built to WebAssembly): ER diagram layout, Compare Structure, dialect-aware migration scripts (SQLite table rebuilds), Laravel and Doctrine migration files, and Explain Plan statements and plan parsing per dialect."
order: 62
tags: [crate, rust, database, diagram, migration, explain plan]
---

# forge-dbschema

`crates/forge-dbschema` — the schema tools of Forge's database support, modelled on
DataGrip: ER diagrams, Compare Structure with migrations, and query plans. Pure code over the
[`forge-dbmodel`](forge-dbmodel.md) types, shared by `forge-db` (desktop) and the
browser-mode backend (WebAssembly). User documentation:
[Database tools](../../user/database.md#er-diagrams).

## Purpose

- `diagram`: the ER diagram of a schema or of selected tables (with the tables they reference
  and that reference them). Nodes are sized for a 12px monospace font (`CHAR_WIDTH`,
  `HEADER_HEIGHT`, `ROW_HEIGHT`; at most `MAX_COLUMNS` columns, long types shortened), laid out
  in layers per connected component — referenced tables above the tables referencing them,
  cycles broken at back edges, layers ordered by barycenter sweeps, each node pulled under the
  tables it references — and tables without relations in a grid below. Foreign keys become
  polylines from the column row to the referenced column row, with lanes so parallel edges stay
  apart. The UI only draws.
- `compare`: `compare(source, target, options) -> SchemaDiff`: tables and views added
  (source only), removed (target only) or changed, with column (type, nullability, default,
  auto increment, comment), primary key, index, foreign key, check, view query and comment
  differences. Types are normalized (`int4` = `integer`), indexes match by name or, when the
  database named them (`sqlite_autoindex_*`), by columns; foreign keys and checks match by
  definition; virtual objects are skipped.
- `migrate`: `migration(dialect, source, target, options) -> Migration`: the statements that
  turn the target into the source, in phases (drop foreign keys and views, drop indexes / keys /
  checks, drop tables, create tables and change columns, add keys / indexes / checks, add
  foreign keys and views) with per-dialect syntax; SQLite tables are rebuilt (create, copy the
  common columns, drop, rename; `PRAGMA foreign_keys` off and on; indexes and triggers created
  again). Drops are marked destructive and can be left out; warnings name what needs a look.
- `framework`: `framework_migration(kind, …) -> MigrationFile`: a Laravel migration (schema
  builder calls; `down()` is the migration back) or a Doctrine Migrations class (`addSql`).
- `plan`: `explain_script(dialect, statement, raw)` — `EXPLAIN (FORMAT JSON)`,
  `EXPLAIN FORMAT=TREE` (falling back to `EXPLAIN` for MariaDB), `EXPLAIN QUERY PLAN`,
  `SET SHOWPLAN_ALL ON/OFF`, ClickHouse `EXPLAIN json = 1`; never `ANALYZE` — and
  `parse_plan(columns, rows) -> Plan`: the format is recognized from the result (JSON plans,
  MySQL tree, the classic MySQL table, SQLite query plan rows, SQL Server SHOWPLAN rows, Oracle
  `PLAN_TABLE` rows, PostgreSQL text) and becomes a `PlanNode` tree with cost, startup cost,
  own cost, rows, details and properties, plus the flame bars (width by cost, rows or equal
  shares).
- `api`: `compare_schemas(CompareRequest) -> CompareResult`, the request of `db_schema_compare`
  and the WebAssembly bridge: each side a schema or a DDL snapshot (parsed with
  `forge_dbmodel::ddl`), the migration and optionally a framework file.

### Decisions

- **Layout in Rust, drawing in the UI**: the layout is deterministic and testable, and the
  same code runs in browser mode; the UI renders SVG and exports it (SVG, PNG).
- **SQLite rebuilds** follow SQLite's documented procedure for changes `ALTER TABLE` cannot
  make; adding a nullable column (or one with a default) and changing plain indexes stay
  `ALTER TABLE` / `CREATE INDEX`.
- **Oracle Explain Plan** is refused: Forge runs each Oracle statement in its own SQL*Plus
  process, so `PLAN_TABLE` rows of `EXPLAIN PLAN FOR` are gone before they can be read. The
  parser still reads `PLAN_TABLE` rows.

## Modules

| Module | Summary |
| --- | --- |
| `diagram.rs` | `DiagramRequest` → `Diagram { nodes, edges, width, height, external_edges }`; `ColumnsMode` (`all`, `keys`, `none`). |
| `compare.rs` | `compare`, `CompareOptions` (`ignoreComments`, `ignoreDefaults`, `ignoreViews`), `SchemaDiff` / `TableDiff` / `ItemDiff`, type normalization and matching helpers. |
| `migrate.rs` | `migration`, `MigrationOptions` (`drops`, `compare`), `Migration { sql, statements, warnings, destructive }`. |
| `framework.rs` | `framework_migration`, `FrameworkKind` (`laravel`, `doctrine`), `MigrationFile { path, text }`. |
| `plan.rs` | `explain_script` → `ExplainScript { statements, result, cleanup, fallback }`; `parse_plan` → `Plan { format, root, total_cost, weight, flame }`. |
| `api.rs` | `CompareRequest`, `CompareSide`, `CompareResult`, `compare_schemas`. |

## Used by

- [`forge-db`](forge-db.md): re-exported as `forge_db::schema`; `DbService::explain` runs the
  explain script in a console session.
- IPC (`app/src-tauri/src/commands/db.rs`): `db_diagram`, `db_schema_compare`, `db_explain`
  (and `db_save_export` for diagram exports).
- Browser mode: [`forge-wasm`](forge-wasm.md) `dbschema_diagram`, `dbschema_compare`,
  `dbschema_explain_script`, `dbschema_parse_plan` (`parsers/dbschema.rs`;
  `app/ui/src/mock/dbSchemaMock.ts`).
- UI: `app/ui/src/features/database/schema` (diagram and compare tabs, the console's Plan tab).

## Dependencies

- Workspace crates: [`forge-dbmodel`](forge-dbmodel.md), [`forge-sql`](forge-sql.md)
- External: `serde`, `serde_json` (MIT or Apache-2.0)

## Tests

6 source files; 21 unit tests: layout (layers, no overlaps, edges between column rows, self
references, cycles, neighbours, column modes, long types), compare (diff tree, normalization,
generated names, views and options), migrations per dialect (PostgreSQL order, MySQL, SQL
Server, Oracle and ClickHouse syntax, no drops, SQLite rebuilds with triggers), Laravel and
Doctrine files, explain statements and every plan format, and a DDL snapshot comparison. The
end-to-end tests are in `forge-db` (`tests/schema_tools.rs`): two real SQLite databases are
compared, the generated migration runs on the target (rows survive the rebuilds) and a second
comparison finds no differences; Explain Plan runs in a console session.

```sh
cargo test -p forge-dbschema
cargo test -p forge-db --test schema_tools
```
