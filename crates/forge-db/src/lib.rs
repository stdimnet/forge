//! Forge database tools (the DataGrip part of Forge).
//!
//! - [`config`]: data sources in `.forge/dataSources.toml` and the global file;
//!   passwords in the OS keychain ([`secrets`], service `forge-db`).
//! - [`env`]: suggestions from Laravel `DB_*` and Symfony `DATABASE_URL`.
//! - [`conn`]: MySQL/MariaDB, PostgreSQL and SQLite pools via `sqlx`; the other engines
//!   ([`engine`]: SQL Server, ClickHouse, Redis, MongoDB, Oracle) behind driver traits.
//! - [`ssh`], [`tls`]: per data source SSH tunnels (opened with `forge-remote`) and SSL/TLS
//!   certificates (client key passphrases in the keychain).
//! - [`introspect`], [`ddl`]: the object tree per dialect (schemas, tables, views,
//!   materialized views, columns, keys, indexes, checks, triggers, routines, sequences,
//!   types, roles) and DDL; the model itself is the pure `forge-dbmodel` crate ([`model`]).
//! - [`DbService`] explorer: cached models per data source (user cache dir), the schema
//!   filter, virtual foreign keys and columns (`.forge/database/virtual.toml`), DDL data
//!   sources, Quick Documentation and full-text search.
//! - [`service::DbService`]: consoles with own or shared sessions, transactions and cancel,
//!   the read-only / production [`guard`], table paging, row editing ([`edit`]) and export
//!   ([`export`]).
//! - Database objects ([`DbService::run_script`], [`DbService::import`],
//!   [`DbService::dump_target`]): scripts of the Create/Modify Table, Drop and Rename
//!   dialogs, data imports and the connection of the native dump tools; the SQL and
//!   command lines come from the pure `forge-dbmodel` and `forge-dbtransfer` crates.
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
pub use forge_dataedit as dataedit;
/// The explorer model (also built to WebAssembly for browser mode).
pub use forge_dbmodel as model;
pub use forge_dbmodel::{DbModel, ObjectDoc, ObjectRef, VirtualObjects};
pub use forge_sql::ExportFormat;
pub use forge_sql::{Guard, GuardPolicy};
pub use introspect::{
    CheckInfo, ColumnInfo, ForeignKey, IndexInfo, RoleInfo, RoutineArg, RoutineInfo, SchemaInfo,
    SchemaName, SequenceInfo, TableInfo, TableKind, TriggerInfo, TypeInfo,
};
pub use secrets::{KeyringStore, MemoryStore, SecretStore};
pub use service::{
    row_key, ApplyResult, ChangePreview, ColumnFilter, DataSourceInfo, DbService, FilterOp,
    ImportError, ImportProgress, ImportSummary, QueryPage, ScriptResult, SearchProgress,
    SearchRequest, SearchScope, SearchSummary, SecretUpdate,
    SessionInfo, SessionMode, SortKey, StatementResult, TableDataRequest, TableMatches, TestResult,
};
pub use split::{split_statements, Statement};
/// Cancels running imports.
pub use tokio_util::sync::CancellationToken;
pub use tunnel::{DbTunnel, ProjectContext, TunnelEndpoint};
