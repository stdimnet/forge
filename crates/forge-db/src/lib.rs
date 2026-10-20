//! Forge database tools (the DataGrip part of Forge).
//!
//! - [`config`]: data sources in `.forge/dataSources.toml` and the global file;
//!   passwords in the OS keychain ([`secrets`], service `forge-db`).
//! - [`env`]: suggestions from Laravel `DB_*` and Symfony `DATABASE_URL`.
//! - [`conn`]: MySQL/MariaDB, PostgreSQL and SQLite pools via `sqlx`.
//! - [`introspect`], [`ddl`]: schemas, tables, columns, keys, indexes, routines, DDL.
//! - [`service::DbService`]: consoles with transactions and cancel, table paging, row
//!   editing ([`edit`]) and export ([`export`]).
//! - [`value`]: cell values as JavaScript-safe JSON.

pub mod config;
pub mod conn;
pub mod ddl;
pub mod edit;
pub mod env;
pub mod error;
pub mod export;
pub mod introspect;
pub mod secrets;
pub mod service;
pub mod split;
pub mod value;

pub use config::{DataSource, DataSourceOptions, Dialect, Driver, Scope, SslMode};
pub use conn::{ColumnMeta, DbPool, RawResult};
pub use edit::{ChangeKind, RowChange};
pub use env::DetectedDataSource;
pub use error::{DbError, DbResult};
pub use export::{export, ExportFormat};
pub use introspect::{
    ColumnInfo, ForeignKey, IndexInfo, RoutineInfo, SchemaInfo, SchemaName, TableInfo, TableKind,
};
pub use secrets::{KeyringStore, MemoryStore, SecretStore};
pub use service::{
    ApplyResult, DataSourceInfo, DbService, QueryPage, SessionInfo, SortKey, StatementResult,
    TableDataRequest, TestResult,
};
pub use split::{split_statements, Statement};
