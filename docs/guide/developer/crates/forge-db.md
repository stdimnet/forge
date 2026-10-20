---
title: forge-db
description: "Forge IDE database tools: data sources, introspection, query execution and data editing."
order: 60
tags: [crate, rust]
---

# forge-db

`crates/forge-db` — Forge IDE database tools: data sources, introspection, query execution and data editing.

## Purpose

Forge database tools (the DataGrip part of Forge).

- `config`: data sources in `.forge/dataSources.toml` and the global file;
  passwords in the OS keychain (`secrets`, service `forge-db`).
- `env`: suggestions from Laravel `DB_*` and Symfony `DATABASE_URL`.
- `conn`: MySQL/MariaDB, PostgreSQL and SQLite pools via `sqlx`.
- `introspect`, `ddl`: schemas, tables, columns, keys, indexes, routines, DDL.
- `service::DbService`: consoles with transactions and cancel, table paging, row
  editing (`edit`) and export (`export`).
- `value`, `split`, `export`: re-exported from the pure `forge-sql` crate.

## Modules

| Module | Summary |
| --- | --- |
| `config.rs` | Data source configuration: `.forge/dataSources.toml` in the project and `<config dir>/forge/dataSources.toml` for global data sources. Passwords never go into these files; they live in the OS keychain (see `crate::secrets`). |
| `conn.rs` | Driver-neutral pools and connections. All SQL is sent as raw text (MySQL text protocol, PostgreSQL simple query protocol), so every statement type works and values arrive in a uniform text form that `crate::value` converts. |
| `ddl.rs` | Table DDL: read from the server where it keeps it (MySQL `SHOW CREATE`, SQLite `sqlite_master`), otherwise generated from introspection (PostgreSQL). |
| `edit.rs` | Row editing from the data grid; the statements are built by `forge_sql::edit`. |
| `env.rs` | Data sources suggested from a project's `.env` files: Laravel's `DB_*` variables and Symfony/Doctrine's `DATABASE_URL`. |
| `error.rs` |  |
| `introspect.rs` | Database metadata: schemas, tables, views, columns, keys, indexes and routines. |
| `secrets.rs` | Data source passwords: the OS keychain (service `forge-db`, account = data source id). |
| `service.rs` | `DbService`: data sources of the open project plus global ones, their pools, console sessions with transaction state, table data paging and row editing. |

## Main public items

```rust
pub use forge_sql::{export, split, value};
pub use config::{DataSource, DataSourceOptions, Dialect, Driver, Scope, SslMode};
pub use conn::{ColumnMeta, DbPool, RawResult};
pub use edit::{ChangeKind, RowChange};
pub use env::DetectedDataSource;
pub use error::{DbError, DbResult};
pub use forge_sql::ExportFormat;
pub use introspect::{…};
pub use secrets::{KeyringStore, MemoryStore, SecretStore};
pub use service::{…};
pub use split::{split_statements, Statement};
```

## Used by

- IPC: `db_data_sources_list`, `db_data_source_save`, `db_data_source_delete`, `db_test_connection`, `db_disconnect`, `db_schemas`, `db_introspect`, `db_table_info`, `db_table_ddl`, `db_table_data`, `db_apply_changes`, `db_session_open`, `db_session_close`, `db_execute`, `db_fetch`, `db_set_auto_commit`, `db_commit`, `db_rollback`, `db_cancel`, `db_detect_env`, `db_add_from_env`, `db_export`, `db_statement_at` (`app/src-tauri/src/commands/db.rs`)
- UI: `app/ui/src/features/database`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-sql`](forge-sql.md)
- External: `dirs`, `futures-util`, `hex`, `keyring`, `serde`, `serde_json`, `sqlx`, `thiserror`, `tokio`, `tokio-util`, `toml`, `tracing`

## Tests

10 source files, about 2838 lines; 11 unit tests and 11 integration tests in `tests/` (`sqlite.rs`).

```sh
cargo test -p forge-db
```
