//! Drivers beyond `sqlx`, each behind the [`EnginePool`] / [`EngineConn`] traits:
//!
//! - [`sqlserver`]: SQL Server with the pure-Rust TDS driver `tiberius` over rustls.
//! - [`clickhouse`]: ClickHouse over its HTTP interface (JSON formats, `reqwest`).
//! - [`redis`]: Redis with `redis-rs`: a keys tree, value viewer and a command console.
//! - [`mongo`]: MongoDB with the official driver: collections, documents as JSON and a
//!   shell-style console (`find`, `aggregate`, …).
//! - [`oracle`]: Oracle through SQL*Plus or SQLcl installed by the user (no client is
//!   bundled), one process per statement.
//!
//! SQL engines are introspected with SQL in [`crate::introspect`]; Redis and MongoDB answer
//! [`EngineConn::schemas`] and [`EngineConn::schema`] themselves.

use std::any::Any;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde_json::Value;

use crate::config::{DataSource, Dialect, Driver};
use crate::conn::RawResult;
use crate::error::{DbError, DbResult};
use crate::introspect::{SchemaInfo, SchemaName};
use crate::tls::TlsMaterial;

pub mod clickhouse;
pub mod mongo;
pub mod oracle;
pub mod redis;
pub mod sqlserver;

/// Stops the statement running on a connection, from another task.
pub type Canceller = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// One connection (or, for HTTP and process based engines, one session).
pub trait EngineConn: Send {
    fn dialect(&self) -> Dialect;

    /// Run one statement and keep at most `max` rows after skipping `skip`.
    fn run<'a>(
        &'a mut self,
        sql: &'a str,
        skip: usize,
        max: usize,
    ) -> BoxFuture<'a, DbResult<RawResult>>;

    fn server_version(&mut self) -> BoxFuture<'_, DbResult<String>>;

    /// Databases of engines that are not introspected with SQL (Redis, MongoDB).
    fn schemas(&mut self) -> BoxFuture<'_, DbResult<Vec<SchemaName>>> {
        Box::pin(async { Err(DbError::Refused("not supported by this driver".into())) })
    }

    /// Contents of a database of engines that are not introspected with SQL.
    fn schema<'a>(
        &'a mut self,
        _schema: &'a str,
        _only: Option<&'a str>,
    ) -> BoxFuture<'a, DbResult<SchemaInfo>> {
        Box::pin(async { Err(DbError::Refused("not supported by this driver".into())) })
    }

    fn canceller(&self) -> Option<Canceller> {
        None
    }

    /// After a cancelled statement the connection is in an unknown state and is replaced.
    fn reconnect_after_cancel(&self) -> bool {
        false
    }

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Opens connections of one data source.
pub trait EnginePool: Send + Sync {
    fn dialect(&self) -> Dialect;
    fn as_any(&self) -> &dyn Any;
    fn acquire(&self) -> BoxFuture<'_, DbResult<Box<dyn EngineConn>>>;
    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

/// What an engine needs to connect: the data source (host and port already pointing at a
/// tunnel when there is one), its secrets and certificates.
pub struct ConnectSpec<'a> {
    pub ds: &'a DataSource,
    pub password: Option<&'a str>,
    pub tls: TlsMaterial,
    /// The host certificates must name: the configured host, also through a tunnel.
    pub tls_host: String,
    pub root: Option<&'a Path>,
}

impl ConnectSpec<'_> {
    pub fn host(&self) -> String {
        match self.ds.host.trim() {
            "" => "localhost".into(),
            h => h.into(),
        }
    }

    pub fn port(&self) -> u16 {
        self.ds.port_or_default()
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.ds.options.connect_timeout_secs.clamp(1, 300) as u64)
    }
}

/// Connect the engine of `spec.ds.driver` (not the `sqlx` drivers).
pub async fn connect(spec: ConnectSpec<'_>) -> DbResult<Arc<dyn EnginePool>> {
    Ok(match spec.ds.driver {
        Driver::Sqlserver => Arc::new(sqlserver::SqlServerPool::connect(&spec).await?),
        Driver::Clickhouse => Arc::new(clickhouse::ClickHousePool::connect(&spec).await?),
        Driver::Redis => Arc::new(redis::RedisPool::connect(&spec).await?),
        Driver::Mongodb => Arc::new(mongo::MongoPool::connect(&spec).await?),
        Driver::Oracle => Arc::new(oracle::OraclePool::connect(&spec)?),
        other => {
            return Err(DbError::Config(format!(
                "{} is not an engine driver",
                other.label()
            )))
        }
    })
}

/// A cell of a nested value (arrays, objects): the grid shows it as JSON text.
pub fn flat_cell(v: Value) -> Value {
    match v {
        Value::Array(_) => Value::String(v.to_string()),
        Value::Object(ref o) if !(o.len() == 1 && o.contains_key("$hex")) => {
            Value::String(v.to_string())
        }
        other => other,
    }
}

/// Keep at most `max` rows after skipping `skip` (engines that return everything at once).
pub fn page_rows(mut rows: Vec<Vec<Value>>, skip: usize, max: usize) -> (Vec<Vec<Value>>, bool) {
    let rest = rows.split_off(skip.min(rows.len()));
    let has_more = rest.len() > max;
    (rest.into_iter().take(max).collect(), has_more)
}

/// Split a console line into words, honouring `"…"` and `'…'` quotes and `\` escapes.
pub fn shell_words(line: &str) -> DbResult<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = line.trim().chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' => {
                in_word = true;
                let q = c;
                loop {
                    match chars.next() {
                        Some('\\') if q == '"' => match chars.next() {
                            Some('n') => cur.push('\n'),
                            Some('t') => cur.push('\t'),
                            Some(x) => cur.push(x),
                            None => break,
                        },
                        Some(x) if x == q => break,
                        Some(x) => cur.push(x),
                        None => return Err(DbError::Sql(format!("unbalanced quotes in: {line}"))),
                    }
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn splits_command_words() {
        assert_eq!(
            shell_words(r#"SET "user:1 name" 'Ada Lovelace' x\y "a\"b""#).unwrap(),
            vec!["SET", "user:1 name", "Ada Lovelace", "x\\y", "a\"b"]
        );
        assert_eq!(shell_words("  GET  a  ").unwrap(), vec!["GET", "a"]);
        assert_eq!(shell_words(r#"SET a """#).unwrap(), vec!["SET", "a", ""]);
        assert!(shell_words("GET \"a").is_err());
    }

    #[test]
    fn flattens_nested_cells_and_pages() {
        assert_eq!(flat_cell(json!([1, 2])), json!("[1,2]"));
        assert_eq!(flat_cell(json!({"$hex": "ff"})), json!({"$hex": "ff"}));
        assert_eq!(flat_cell(json!({"a": 1})), json!("{\"a\":1}"));
        let rows: Vec<Vec<Value>> = (0..5).map(|i| vec![json!(i)]).collect();
        let (page, more) = page_rows(rows.clone(), 1, 2);
        assert_eq!(page, vec![vec![json!(1)], vec![json!(2)]]);
        assert!(more);
        let (page, more) = page_rows(rows, 4, 2);
        assert_eq!(page.len(), 1);
        assert!(!more);
    }
}
