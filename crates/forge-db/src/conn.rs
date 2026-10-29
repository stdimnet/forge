//! Driver-neutral pools and connections. All SQL is sent as raw text (MySQL text protocol,
//! PostgreSQL simple query protocol), so every statement type works and values arrive in
//! a uniform text form that [`crate::value`] converts. The other engines (SQL Server,
//! ClickHouse, Redis, MongoDB, Oracle) sit behind the traits of [`crate::engine`].

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use sqlx::mysql::{
    MySql, MySqlConnectOptions, MySqlPool, MySqlPoolOptions, MySqlRow, MySqlSslMode,
};
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgRow, PgSslMode, Postgres};
use sqlx::sqlite::{Sqlite, SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Decode, Either, Executor, Row, TypeInfo, ValueRef};

use crate::config::{DataSource, Dialect, Driver, SslMode};
use crate::engine::{self, ConnectSpec, EngineConn, EnginePool};
use crate::error::{DbError, DbResult};
use crate::tls::TlsMaterial;
use crate::value;

/// Name and driver type of a result column.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,
}

/// Rows of one statement (at most the requested page) and its update count.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawResult {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: u64,
    /// More rows exist after this page.
    pub has_more: bool,
    /// MongoDB: the documents of the rows as relaxed extended JSON (the JSON view).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<Value>,
}

/// A connection pool for one data source.
#[derive(Clone)]
pub enum DbPool {
    MySql(MySqlPool),
    Postgres(PgPool),
    Sqlite(SqlitePool),
    Engine(Arc<dyn EnginePool>),
}

/// A connection checked out of a [`DbPool`].
pub enum Conn {
    MySql(PoolConnection<MySql>),
    Postgres(PoolConnection<Postgres>),
    Sqlite(PoolConnection<Sqlite>),
    Engine(Box<dyn EngineConn>),
}

const MAX_CONNECTIONS: u32 = 5;

fn mysql_options(
    ds: &DataSource,
    password: Option<&str>,
    tls: &TlsMaterial,
) -> MySqlConnectOptions {
    let host = if ds.host.is_empty() {
        "localhost"
    } else {
        &ds.host
    };
    let mut o = MySqlConnectOptions::new()
        .host(host)
        .port(ds.port_or_default())
        .ssl_mode(match ds.options.ssl_mode {
            SslMode::Disable => MySqlSslMode::Disabled,
            SslMode::Prefer => MySqlSslMode::Preferred,
            SslMode::Require => MySqlSslMode::Required,
            SslMode::VerifyCa => MySqlSslMode::VerifyCa,
            SslMode::VerifyFull => MySqlSslMode::VerifyIdentity,
        });
    if ds.options.ssl_mode.enabled() {
        if let Some(ca) = &tls.ca {
            o = o.ssl_ca_from_pem(ca.clone());
        }
        if let (Some(c), Some(k)) = (&tls.cert, &tls.key) {
            o = o.ssl_client_cert_from_pem(c).ssl_client_key_from_pem(k);
        }
    }
    if !ds.user.is_empty() {
        o = o.username(&ds.user);
    }
    if let Some(p) = password.filter(|p| !p.is_empty()) {
        o = o.password(p);
    }
    if !ds.database.is_empty() {
        o = o.database(&ds.database);
    }
    o
}

fn pg_options(ds: &DataSource, password: Option<&str>, tls: &TlsMaterial) -> PgConnectOptions {
    let host = if ds.host.is_empty() {
        "localhost"
    } else {
        &ds.host
    };
    let mut o = PgConnectOptions::new()
        .host(host)
        .port(ds.port_or_default())
        .application_name("Forge")
        .ssl_mode(match ds.options.ssl_mode {
            SslMode::Disable => PgSslMode::Disable,
            SslMode::Prefer => PgSslMode::Prefer,
            SslMode::Require => PgSslMode::Require,
            SslMode::VerifyCa => PgSslMode::VerifyCa,
            SslMode::VerifyFull => PgSslMode::VerifyFull,
        });
    if ds.options.ssl_mode.enabled() {
        if let Some(ca) = &tls.ca {
            o = o.ssl_root_cert_from_pem(ca.clone());
        }
        if let (Some(c), Some(k)) = (&tls.cert, &tls.key) {
            o = o.ssl_client_cert_from_pem(c).ssl_client_key_from_pem(k);
        }
    }
    if !ds.user.is_empty() {
        o = o.username(&ds.user);
    }
    if let Some(p) = password.filter(|p| !p.is_empty()) {
        o = o.password(p);
    }
    if !ds.database.is_empty() {
        o = o.database(&ds.database);
    }
    o
}

fn sqlite_options(ds: &DataSource, root: Option<&Path>) -> DbResult<SqliteConnectOptions> {
    if ds.file.trim().is_empty() {
        return Err(DbError::Config("no database file set".into()));
    }
    let o = if ds.file == ":memory:" {
        SqliteConnectOptions::from_str("sqlite::memory:")?
    } else {
        SqliteConnectOptions::new()
            .filename(ds.sqlite_path(root))
            .create_if_missing(!ds.options.read_only)
    };
    Ok(o.read_only(ds.options.read_only).foreign_keys(true))
}

/// Statements run on each new connection: read-only mode and the user's startup script.
fn init_statements(ds: &DataSource) -> Vec<String> {
    let mut out = Vec::new();
    if ds.options.read_only {
        match ds.dialect() {
            Dialect::MySql => out.push("SET SESSION TRANSACTION READ ONLY".into()),
            Dialect::Postgres => {
                out.push("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY".into())
            }
            _ => {}
        }
    }
    if !ds.options.init_sql.trim().is_empty() {
        out.push(ds.options.init_sql.clone());
    }
    out
}

impl DbPool {
    /// Open a pool (without certificates or SSL secrets) and check that a connection can
    /// be made.
    pub async fn connect(
        ds: &DataSource,
        password: Option<&str>,
        root: Option<&Path>,
    ) -> DbResult<Self> {
        Self::open(ConnectSpec {
            ds,
            password,
            tls: TlsMaterial::default(),
            tls_host: ds.host.clone(),
            root,
        })
        .await
    }

    /// Open a pool for `spec` and check that a connection can be made.
    pub async fn open(spec: ConnectSpec<'_>) -> DbResult<Self> {
        let (ds, password, root) = (spec.ds, spec.password, spec.root);
        if !matches!(
            ds.driver,
            Driver::Mysql | Driver::Mariadb | Driver::Postgres | Driver::Sqlite
        ) {
            return Ok(DbPool::Engine(engine::connect(spec).await?));
        }
        let tls = &spec.tls;
        let timeout = Duration::from_secs(ds.options.connect_timeout_secs.clamp(1, 300) as u64);
        let init = init_statements(ds);
        macro_rules! init_hook {
            () => {{
                let init = init.clone();
                move |conn, _meta| {
                    let init = init.clone();
                    Box::pin(async move {
                        for sql in &init {
                            conn.execute(sqlx::raw_sql(sql)).await?;
                        }
                        Ok(())
                    })
                }
            }};
        }
        let pool = match ds.driver {
            Driver::Mysql | Driver::Mariadb => DbPool::MySql(
                MySqlPoolOptions::new()
                    .max_connections(MAX_CONNECTIONS)
                    .acquire_timeout(timeout)
                    .after_connect(init_hook!())
                    .connect_with(mysql_options(ds, password, tls))
                    .await?,
            ),
            Driver::Postgres => DbPool::Postgres(
                PgPoolOptions::new()
                    .max_connections(MAX_CONNECTIONS)
                    .acquire_timeout(timeout)
                    .after_connect(init_hook!())
                    .connect_with(pg_options(ds, password, tls))
                    .await?,
            ),
            Driver::Sqlite => DbPool::Sqlite(
                SqlitePoolOptions::new()
                    // An in-memory database exists once per connection.
                    .max_connections(if ds.file == ":memory:" {
                        1
                    } else {
                        MAX_CONNECTIONS
                    })
                    .acquire_timeout(timeout)
                    .after_connect(init_hook!())
                    .connect_with(sqlite_options(ds, root)?)
                    .await?,
            ),
            _ => unreachable!("engine drivers are handled above"),
        };
        Ok(pool)
    }

    pub fn dialect(&self) -> Dialect {
        match self {
            DbPool::MySql(_) => Dialect::MySql,
            DbPool::Postgres(_) => Dialect::Postgres,
            DbPool::Sqlite(_) => Dialect::Sqlite,
            DbPool::Engine(e) => e.dialect(),
        }
    }

    pub async fn acquire(&self) -> DbResult<Conn> {
        Ok(match self {
            DbPool::MySql(p) => Conn::MySql(p.acquire().await?),
            DbPool::Postgres(p) => Conn::Postgres(p.acquire().await?),
            DbPool::Sqlite(p) => Conn::Sqlite(p.acquire().await?),
            DbPool::Engine(e) => Conn::Engine(e.acquire().await?),
        })
    }

    pub async fn close(&self) {
        match self {
            DbPool::MySql(p) => p.close().await,
            DbPool::Postgres(p) => p.close().await,
            DbPool::Sqlite(p) => p.close().await,
            DbPool::Engine(e) => e.close().await,
        }
    }

    /// The engine pool behind a non-`sqlx` driver.
    pub fn engine(&self) -> Option<&Arc<dyn EnginePool>> {
        match self {
            DbPool::Engine(e) => Some(e),
            _ => None,
        }
    }
}

fn text_cell<DB>(row: &DB::Row, i: usize, dialect: Dialect) -> Value
where
    DB: sqlx::Database,
    usize: sqlx::ColumnIndex<DB::Row>,
    for<'r> &'r [u8]: Decode<'r, DB>,
{
    let Ok(raw) = row.try_get_raw(i) else {
        return Value::Null;
    };
    if raw.is_null() {
        return Value::Null;
    }
    let type_name = raw.type_info().name().to_string();
    match <&[u8] as Decode<DB>>::decode(raw) {
        Ok(bytes) => value::from_text(&type_name, dialect, Some(bytes)),
        Err(_) => Value::Null,
    }
}

fn mysql_cell(row: &MySqlRow, i: usize) -> Value {
    text_cell::<MySql>(row, i, Dialect::MySql)
}

fn pg_cell(row: &PgRow, i: usize) -> Value {
    text_cell::<Postgres>(row, i, Dialect::Postgres)
}

fn sqlite_cell(row: &SqliteRow, i: usize) -> Value {
    let raw = || row.try_get_raw(i).ok();
    let Some(r) = raw() else {
        return Value::Null;
    };
    if r.is_null() {
        return Value::Null;
    }
    let storage = r.type_info().name().to_string();
    let dec = match storage.as_str() {
        "INTEGER" => raw()
            .and_then(|r| <i64 as Decode<Sqlite>>::decode(r).ok())
            .map(value::int),
        "REAL" => raw()
            .and_then(|r| <f64 as Decode<Sqlite>>::decode(r).ok())
            .map(value::float),
        "BLOB" => raw()
            .and_then(|r| <Vec<u8> as Decode<Sqlite>>::decode(r).ok())
            .map(|b| value::binary(&b)),
        _ => None,
    };
    dec.or_else(|| {
        raw()
            .and_then(|r| <String as Decode<Sqlite>>::decode(r).ok())
            .map(Value::String)
    })
    .unwrap_or(Value::Null)
}

fn columns_of<R: Row>(row: &R) -> Vec<ColumnMeta> {
    row.columns()
        .iter()
        .map(|c| ColumnMeta {
            name: c.name().to_string(),
            type_name: c.type_info().name().to_string(),
        })
        .collect()
}

/// Run one statement, skip `skip` rows and keep at most `max` rows.
macro_rules! run_on {
    ($conn:expr, $sql:expr, $skip:expr, $max:expr, $cell:expr) => {{
        let mut out = RawResult::default();
        let mut seen = 0usize;
        let mut stream = $conn.fetch_many(sqlx::raw_sql($sql));
        while let Some(item) = stream.next().await {
            match item? {
                Either::Left(done) => out.rows_affected += done.rows_affected(),
                Either::Right(row) => {
                    if out.columns.is_empty() {
                        out.columns = columns_of(&row);
                    }
                    seen += 1;
                    if seen <= $skip {
                        continue;
                    }
                    if out.rows.len() >= $max {
                        out.has_more = true;
                        break;
                    }
                    let cells = (0..row.len()).map(|i| $cell(&row, i)).collect();
                    out.rows.push(cells);
                }
            }
        }
        drop(stream);
        Ok(out)
    }};
}

macro_rules! describe_on {
    ($conn:expr, $sql:expr) => {{
        let d = $conn.describe($sql).await?;
        Ok(d.columns()
            .iter()
            .map(|c| ColumnMeta {
                name: c.name().to_string(),
                type_name: c.type_info().name().to_string(),
            })
            .collect())
    }};
}

impl Conn {
    pub fn dialect(&self) -> Dialect {
        match self {
            Conn::MySql(_) => Dialect::MySql,
            Conn::Postgres(_) => Dialect::Postgres,
            Conn::Sqlite(_) => Dialect::Sqlite,
            Conn::Engine(e) => e.dialect(),
        }
    }

    /// Run `sql` (one statement) and return at most `max` rows after skipping `skip`.
    pub async fn run(&mut self, sql: &str, skip: usize, max: usize) -> DbResult<RawResult> {
        match self {
            Conn::MySql(c) => run_on!(&mut **c, sql, skip, max, mysql_cell),
            Conn::Postgres(c) => run_on!(&mut **c, sql, skip, max, pg_cell),
            Conn::Sqlite(c) => run_on!(&mut **c, sql, skip, max, sqlite_cell),
            Conn::Engine(e) => e.run(sql, skip, max).await,
        }
    }

    /// Run a statement for its side effects; returns the number of affected rows.
    pub async fn execute(&mut self, sql: &str) -> DbResult<u64> {
        Ok(self.run(sql, 0, 0).await?.rows_affected)
    }

    /// Column names and types of a query without running it (for empty results).
    pub async fn describe(&mut self, sql: &str) -> DbResult<Vec<ColumnMeta>> {
        match self {
            Conn::MySql(c) => describe_on!(&mut **c, sql),
            Conn::Postgres(c) => describe_on!(&mut **c, sql),
            Conn::Sqlite(c) => describe_on!(&mut **c, sql),
            Conn::Engine(_) => Ok(Vec::new()),
        }
    }

    /// All rows of a metadata query as optional strings.
    pub async fn text_rows(&mut self, sql: &str) -> DbResult<Vec<Vec<Option<String>>>> {
        let res = self.run(sql, 0, usize::MAX).await?;
        Ok(res
            .rows
            .into_iter()
            .map(|r| {
                r.into_iter()
                    .map(|v| match v {
                        Value::Null => None,
                        Value::String(s) => Some(s),
                        Value::Bool(b) => Some(if b { "1".into() } else { "0".into() }),
                        Value::Object(_) => {
                            value::as_binary(&v).map(|b| String::from_utf8_lossy(&b).into_owned())
                        }
                        other => Some(other.to_string()),
                    })
                    .collect()
            })
            .collect())
    }

    /// The server-side id used to cancel a running statement from another connection.
    pub async fn backend_id(&mut self) -> Option<i64> {
        let sql = match self {
            Conn::MySql(_) => "SELECT CONNECTION_ID()",
            Conn::Postgres(_) => "SELECT pg_backend_pid()",
            Conn::Sqlite(_) | Conn::Engine(_) => return None,
        };
        let rows = self.text_rows(sql).await.ok()?;
        rows.first()?.first()?.as_deref()?.parse().ok()
    }

    /// Server version string.
    pub async fn server_version(&mut self) -> DbResult<String> {
        if let Conn::Engine(e) = self {
            return e.server_version().await;
        }
        let sql = match self {
            Conn::MySql(_) => "SELECT VERSION()",
            Conn::Postgres(_) => "SHOW server_version",
            Conn::Sqlite(_) => "SELECT sqlite_version()",
            Conn::Engine(_) => unreachable!(),
        };
        let rows = self.text_rows(sql).await?;
        Ok(rows
            .first()
            .and_then(|r| r.first().cloned().flatten())
            .unwrap_or_default())
    }
}

/// Statement that cancels the statement running on connection `id`.
pub fn cancel_statement(dialect: Dialect, id: i64) -> Option<String> {
    match dialect {
        Dialect::MySql => Some(format!("KILL QUERY {id}")),
        Dialect::Postgres => Some(format!("SELECT pg_cancel_backend({id})")),
        _ => None,
    }
}
