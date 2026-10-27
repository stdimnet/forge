---
title: forge-db
description: "Forge IDE database tools: data sources (SSH tunnels, SSL, DDL files), drivers for nine engines, the Database Explorer (full object tree, cached models, schema filter, virtual foreign keys, full-text search), consoles with sessions, the read-only / production guard and data editing."
order: 60
tags: [crate, rust]
---

# forge-db

`crates/forge-db` — Forge IDE database tools: data sources, introspection, query execution and data editing.

## Purpose

Forge database tools (the DataGrip part of Forge).

- `config`: data sources in `.forge/dataSources.toml` and the global file (drivers, SSL mode
  and certificate files, SSH tunnel, colour, single session mode); secrets in the OS keychain
  (`secrets`, service `forge-db`: `<id>`, `<id>#ssh`, `<id>#ssl-key`).
- `env`: suggestions from Laravel `DB_*` and Symfony `DATABASE_URL`.
- `conn`: MySQL/MariaDB, PostgreSQL and SQLite pools via `sqlx`; the other engines behind the
  `engine::EnginePool` / `engine::EngineConn` traits.
- `engine`: SQL Server (`tiberius`, rustls), ClickHouse (HTTP + `JSONCompact`, `reqwest`),
  Redis (`redis-rs`: keys tree, value viewer, command console), MongoDB (official driver:
  collections, documents as JSON, shell-style console), Oracle (SQL*Plus / SQLcl on `PATH`,
  one process per statement).
- `ssh`: per data source SSH tunnels opened with `forge-remote` (saved configuration or ad-hoc,
  `known_hosts` confirmation); `tls`: certificate files, encrypted PKCS#8 keys decrypted in
  memory.
- `introspect`, `ddl`: the full object tree per dialect — schemas, tables, views, materialized
  views, columns (identity, generated, comments), keys, foreign keys, indexes, checks,
  triggers, routines with arguments, sequences, types, and server objects (connected database,
  other databases, roles and users) — and DDL. Every catalog query starts with a
  `/* forge:<dialect>.<what> */` tag; optional catalogs (checks, triggers, roles, …) are left
  out when they fail. The model types are the pure [`forge-dbmodel`](forge-dbmodel.md)
  (`forge_db::model`).
- `service::explorer`: the explorer model per data source, cached in memory and in
  `<cache dir>/forge/db-models/` (the tree shows without connecting), the schema filter applied
  when introspecting (hidden schemas are refused), incremental refresh of a schema or table,
  virtual foreign keys and columns (`.forge/database/virtual.toml` through the project files),
  DDL data sources (`Driver::Ddl`: project files and folders parsed in natural order,
  re-parsed when the watcher reports a change) and Quick Documentation.
- `service::search`: full-text search across tables with progress per table and cancel.
- `service::DbService`: consoles with own or shared sessions, transactions and cancel, the
  Sessions view, the read-only / production `guard`, table paging, row editing (`edit`),
  export (`export`) and the Redis keys and values.
- `value`, `split`, `export`, `guard`: re-exported from the pure `forge-sql` crate.

### Decisions

- **Oracle** runs through the user's SQL*Plus or SQLcl: no permissive pure-Rust driver speaks
  Oracle's protocol and the Oracle client must not be bundled. The password goes through the
  process's stdin; output is CSV between markers. No transactions across statements.
- **ODBC** is not supported: generic ODBC needs a system driver manager and native drivers;
  the engines are supported natively instead.
- Writes on read-only data sources are refused twice: by the guard before anything is sent, and
  by the driver (session `READ ONLY`, `readonly=2`, command allow-lists).

## Modules

| Module | Summary |
| --- | --- |
| `config.rs` | Data source configuration: `.forge/dataSources.toml` in the project and `<config dir>/forge/dataSources.toml` for global data sources. Passwords never go into these files; they live in the OS keychain (see `crate::secrets`). |
| `conn.rs` | Driver-neutral pools and connections. All SQL is sent as raw text (MySQL text protocol, PostgreSQL simple query protocol), so every statement type works and values arrive in a uniform text form that `crate::value` converts. Other engines are `DbPool::Engine` / `Conn::Engine`. |
| `ddl.rs` | Table DDL: read from the server where it keeps it (MySQL and ClickHouse `SHOW CREATE`, SQLite `sqlite_master`, Oracle `DBMS_METADATA`), otherwise generated from introspection (PostgreSQL, SQL Server tables). |
| `edit.rs` | Row editing from the data grid; the statements are built by `forge_sql::edit`. |
| `engine/mod.rs` | The driver traits, connect dispatch and shared helpers (paging, command words). |
| `engine/sqlserver.rs` | SQL Server with `tiberius` over rustls: TDS config from the data source (encryption per SSL mode, CA, client certificate, Azure redirects), TDS values as cells. |
| `engine/clickhouse.rs` | ClickHouse over HTTP: `JSONCompact`, `session_id` per console, `readonly=2`, `KILL QUERY` by `query_id`. |
| `engine/redis.rs` | Redis: databases from `INFO keyspace`, `SCAN` with type and TTL, values per type, console replies as rows; read-only allow-list. |
| `engine/mongo.rs` | MongoDB: relaxed-JSON arguments, `find`/`aggregate`/… calls with cursor chains, documents as rows and relaxed extended JSON, fields sampled per collection. |
| `engine/oracle.rs` | Oracle through SQL*Plus / SQLcl: the script (CSV markup, login on stdin), CSV and feedback parsing, `ORA-` errors. |
| `env.rs` | Data sources suggested from a project's `.env` files: Laravel's `DB_*` variables and Symfony/Doctrine's `DATABASE_URL`. |
| `error.rs` |  |
| `introspect.rs` | Database metadata per dialect: schemas, tables, views, materialized views, columns, keys, foreign keys, indexes, checks, triggers, routines with arguments, sequences, types and server objects, with tagged catalog queries. |
| `secrets.rs` | Data source secrets in the OS keychain (service `forge-db`): password (`<id>`), SSH secret (`<id>#ssh`), SSL key passphrase (`<id>#ssl-key`). |
| `service.rs` | `DbService`: data sources of the open project plus global ones, their pools (through SSH tunnels), own and shared console sessions with transaction state, the guard, table data paging with the data editor's filter and sort (SQL by `forge-dataedit`), the DML preview and row editing (by primary or unique key, production writes confirmed) and the Redis keys tree. |
| `service/explorer.rs` | The Database Explorer: cached models, schema filter, incremental refresh, virtual objects, DDL data sources, Quick Documentation. |
| `service/search.rs` | Full-text search: targets from scopes (data sources, schemas, table patterns), one read-only query per table, progress callback, cancel. |
| `ssh.rs` | The SSH tab: `SshConnector` / `ForgeSsh` open a `forge-remote` session (saved configuration or ad-hoc, `~/.ssh/config`, `known_hosts`) and forward a local port. |
| `tls.rs` | Certificate files of the SSL tab, resolved against the project; encrypted PKCS#8 client keys decrypted in memory. |
| `tunnel.rs` | Data sources of a project on a remote host connect through its SSH connection: the database host and port are as the remote host sees them (`localhost` is the remote machine), reached through a local port forwarded with a `direct-tcpip` channel. |

## Main public items

```rust
pub use forge_sql::{export, guard, split, value};
pub use config::{DataModel, DataSource, DataSourceOptions, Dialect, Driver, Scope, SshTunnel, SslFiles, SslMode};
pub use engine::redis::{RedisKey, RedisKeys, RedisValue};
pub use forge_sql::{Guard, GuardPolicy};
pub use conn::{ColumnMeta, DbPool, RawResult};
pub use edit::{ChangeKind, RowChange};
pub use env::DetectedDataSource;
pub use error::{DbError, DbResult};
pub use forge_sql::ExportFormat;
pub use introspect::{…};
pub use secrets::{KeyringStore, MemoryStore, SecretStore};
pub use service::{…};
pub use split::{split_statements, Statement};
pub use tunnel::{DbTunnel, ProjectContext, TunnelEndpoint};
```

## Used by

- IPC (`app/src-tauri/src/commands/db_objects.rs`): `db_objects_guard`, `db_run_script`,
  `db_design_load`, `db_generate_ddl`, `db_import_run`, `db_import_cancel`, `db_dump_tools`,
  `db_dump_command`, `db_restore_guard`, `db_dump_start`
- IPC: `db_model`, `db_object_doc`, `db_virtual_objects`, `db_virtual_objects_save`, `db_match_schemas`, `db_search`, `db_search_cancel`, `db_data_sources_list`, `db_data_source_save`, `db_data_source_delete`, `db_test_connection`, `db_disconnect`, `db_schemas`, `db_introspect`, `db_table_info`, `db_table_ddl`, `db_table_data`, `db_preview_changes`, `db_apply_changes`, `db_session_open`, `db_session_close`, `db_sessions_list`, `db_session_terminate`, `db_guard`, `db_redis_keys`, `db_redis_value`, `db_execute`, `db_fetch`, `db_set_auto_commit`, `db_commit`, `db_rollback`, `db_cancel`, `db_detect_env`, `db_add_from_env`, `db_export`, `db_statement_at` (`app/src-tauri/src/commands/db.rs`)
- UI: `app/ui/src/features/database`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-dataedit`](forge-dataedit.md), [`forge-dbmodel`](forge-dbmodel.md), [`forge-dbtransfer`](forge-dbtransfer.md), [`forge-remote`](forge-remote.md), [`forge-sql`](forge-sql.md)
- External: `bson` (3, without `serde_json`'s `preserve_order`), `chrono`, `dirs`, `futures-util`, `hex`, `keyring`, `mongodb`, `pkcs8`, `redis`, `reqwest`, `rustls`, `serde`, `serde_json`, `sqlx`, `thiserror`, `tiberius`, `tokio`, `tokio-util`, `toml`, `tracing` (all MIT, Apache-2.0 or BSD-3-Clause)

## Tests

21 source files; 30 unit tests and 33 integration tests in `tests/`: `sqlite.rs` (a real SQLite file: sessions, transactions, guard), `explorer.rs` (a real SQLite file: object tree with checks, triggers and views, the model cache, incremental table refresh, schema filter, virtual objects in the project, full-text search, DDL data sources over migration folders), `catalog.rs` (MySQL, PostgreSQL, SQL Server, Oracle and ClickHouse introspection against recorded catalog rows in `tests/fixtures/catalog/<dialect>.json`, keyed by query tag; every recorded query must be asked), `clickhouse.rs` (a local HTTP mock), `redis.rs` (an in-process RESP fake), `ssh_tunnel.rs` (an in-process russh bastion forwarding to the ClickHouse mock), `oracle.rs` (a fake `sqlplus` script, Unix), `remote.rs`, `objects.rs` (a real SQLite file: Create/Modify Table scripts including a rebuild that keeps the rows, Drop, errors naming the failed statement, read-only refused and production confirmed, imports with skip / abort (rolled back) / cancel and progress, the native tool target). No test needs a network or a real server; SQL Server and MongoDB are covered by unit tests of their configuration, value conversion and parsing.

```sh
cargo test -p forge-db
```
