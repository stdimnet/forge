//! Forge database tools (the DataGrip part of Forge).
//!
//! - [`config`]: data sources in `.forge/dataSources.toml` and the global file;
//!   passwords in the OS keychain ([`secrets`], service `forge-db`).
//! - [`env`]: suggestions from Laravel `DB_*` and Symfony `DATABASE_URL`.
//! - [`conn`]: MySQL/MariaDB, PostgreSQL and SQLite pools via `sqlx`; the other engines
//!   ([`engine`]: SQL Server, ClickHouse, Redis, MongoDB, Oracle) behind driver traits.
//! - [`ssh`], [`tls`]: per data source SSH tunnels (opened with `forge-remote`) and SSL/TLS
//!   certificates (client key passphrases in the keychain).
//! - [`introspect`], [`ddl`]: schemas, tables, columns, keys, indexes, routines, DDL.
//! - [`service::DbService`]: consoles with own or shared sessions, transactions and cancel,
//!   the read-only / production [`guard`], table paging, row editing ([`edit`]) and export
//!   ([`export`]).
//! - [`value`], [`split`], [`export`], [`guard`]: re-exported from the pure `forge-sql` crate.

pub mod config;
pub mod conn;
pub mod ddl;
pub mod edit;
pub mod engine;
pub mod env;
pub mod error;
pub mod introspect;
pub mod secrets;
pub mod service;
pub mod ssh;
pub mod tls;
pub mod tunnel;

/// Pure SQL text handling lives in `forge-sql` (also used by the browser-mode UI).
pub use forge_sql::{export, guard, split, value};

pub use config::{
    DataModel, DataSource, DataSourceOptions, Dialect, Driver, Scope, SshTunnel, SslFiles, SslMode,
};
pub use conn::{ColumnMeta, DbPool, RawResult};
pub use edit::{ChangeKind, RowChange};
pub use engine::redis::{RedisKey, RedisKeys, RedisValue};
pub use env::DetectedDataSource;
pub use error::{DbError, DbResult};
pub use forge_sql::ExportFormat;
pub use forge_sql::{Guard, GuardPolicy};
pub use introspect::{
    ColumnInfo, ForeignKey, IndexInfo, RoutineInfo, SchemaInfo, SchemaName, TableInfo, TableKind,
};
pub use secrets::{KeyringStore, MemoryStore, SecretStore};
pub use service::{
    ApplyResult, DataSourceInfo, DbService, QueryPage, SecretUpdate, SessionInfo, SessionMode,
    SortKey, StatementResult, TableDataRequest, TestResult,
};
pub use split::{split_statements, Statement};
pub use tunnel::{DbTunnel, ProjectContext, TunnelEndpoint};
