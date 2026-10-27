//! SQL Server and Azure SQL with `tiberius` (pure-Rust TDS, MIT OR Apache-2.0) over rustls.
//! Every console session is its own TDS connection; metadata queries open short-lived ones.

use std::any::Any;
use std::time::Duration;

use futures_util::future::BoxFuture;
use futures_util::TryStreamExt;
use serde_json::Value;
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, FromSql, QueryItem};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::{ConnectSpec, EngineConn, EnginePool};
use crate::config::{Dialect, SslMode};
use crate::conn::{ColumnMeta, RawResult};
use crate::error::{DbError, DbResult};
use crate::split::leading_keyword;
use crate::value;

type Tds = Client<Compat<TcpStream>>;

pub struct SqlServerPool {
    config: Config,
    timeout: Duration,
    init: Vec<String>,
}

/// TDS settings of a data source: login, database, application name and encryption.
pub fn config_for(spec: &ConnectSpec<'_>) -> DbResult<Config> {
    let ds = spec.ds;
    let mut c = Config::new();
    c.host(spec.host());
    c.port(spec.port());
    c.application_name("Forge");
    c.authentication(AuthMethod::sql_server(
        ds.user.trim(),
        spec.password.unwrap_or_default(),
    ));
    if !ds.database.trim().is_empty() {
        c.database(ds.database.trim());
    }
    // Read-only data sources declare ApplicationIntent=ReadOnly; the console guard refuses
    // writes before they reach the server.
    c.readonly(ds.options.read_only);
    match ds.options.ssl_mode {
        SslMode::Disable => c.encryption(EncryptionLevel::NotSupported),
        // The login is encrypted; the server decides about the rest.
        SslMode::Prefer => {
            c.encryption(EncryptionLevel::Off);
            c.trust_cert();
        }
        SslMode::Require => {
            c.encryption(EncryptionLevel::Required);
            c.trust_cert();
        }
        SslMode::VerifyCa | SslMode::VerifyFull => {
            c.encryption(EncryptionLevel::Required);
            if let Some(ca) = &spec.tls.ca {
                c.trust_cert_ca_bundle(ca.clone());
            }
            // Through a tunnel the certificate still names the database host.
            c.hostname_in_certificate(&spec.tls_host);
        }
    }
    if !ds.options.ssl.cert_file.is_empty() || !ds.options.ssl.key_file.is_empty() {
        if ds.options.ssl.cert_file.is_empty() || ds.options.ssl.key_file.is_empty() {
            return Err(DbError::Config(
                "SQL Server client certificates need both a certificate and a key file".into(),
            ));
        }
        let key_path = crate::tls::resolve(&ds.options.ssl.key_file, spec.root);
        if std::fs::read_to_string(&key_path).is_ok_and(|k| k.contains("ENCRYPTED")) {
            return Err(DbError::Config(
                "SQL Server reads the client key from its file: use an unencrypted key".into(),
            ));
        }
        c.client_certificate(
            crate::tls::resolve(&ds.options.ssl.cert_file, spec.root),
            key_path,
        );
    }
    Ok(c)
}

impl SqlServerPool {
    pub async fn connect(spec: &ConnectSpec<'_>) -> DbResult<Self> {
        let pool = Self {
            config: config_for(spec)?,
            timeout: spec.timeout(),
            init: spec
                .ds
                .options
                .init_sql
                .trim()
                .is_empty()
                .then(Vec::new)
                .unwrap_or_else(|| vec![spec.ds.options.init_sql.clone()]),
        };
        // Check the login now so "Test Connection" and the tree report problems.
        drop(pool.open().await?);
        Ok(pool)
    }

    async fn open(&self) -> DbResult<SqlServerConn> {
        let mut client = connect_tds(self.config.clone(), self.timeout).await?;
        for sql in &self.init {
            client
                .simple_query(sql.as_str())
                .await?
                .into_results()
                .await?;
        }
        Ok(SqlServerConn { client })
    }
}

async fn tcp(addr: String, timeout: Duration) -> DbResult<TcpStream> {
    let tcp = tokio::time::timeout(timeout, TcpStream::connect(&addr))
        .await
        .map_err(|_| {
            DbError::Sql(format!(
                "connection to {addr} timed out (is the server running?)"
            ))
        })?
        .map_err(|e| DbError::Sql(format!("cannot connect to {addr}: {e}")))?;
    let _ = tcp.set_nodelay(true);
    Ok(tcp)
}

async fn connect_tds(mut config: Config, timeout: Duration) -> DbResult<Tds> {
    let stream = tcp(config.get_addr(), timeout).await?;
    let login = tokio::time::timeout(
        timeout,
        Client::connect(config.clone(), stream.compat_write()),
    );
    match login.await {
        Err(_) => Err(DbError::Sql("SQL Server login timed out".into())),
        Ok(Ok(c)) => Ok(c),
        // Azure SQL redirects to the database's node.
        Ok(Err(tiberius::error::Error::Routing { host, port })) => {
            config.host(&host);
            config.port(port);
            let stream = tcp(config.get_addr(), timeout).await?;
            Ok(Client::connect(config, stream.compat_write()).await?)
        }
        Ok(Err(e)) => Err(e.into()),
    }
}

impl EnginePool for SqlServerPool {
    fn dialect(&self) -> Dialect {
        Dialect::SqlServer
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn acquire(&self) -> BoxFuture<'_, DbResult<Box<dyn EngineConn>>> {
        Box::pin(async move { Ok(Box::new(self.open().await?) as Box<dyn EngineConn>) })
    }
}

pub struct SqlServerConn {
    client: Tds,
}

/// A TDS value as a grid cell.
pub fn cell(data: &ColumnData<'static>) -> Value {
    let v = match data {
        ColumnData::U8(v) => v.map(|x| value::int(x as i64)),
        ColumnData::I16(v) => v.map(|x| value::int(x as i64)),
        ColumnData::I32(v) => v.map(|x| value::int(x as i64)),
        ColumnData::I64(v) => v.map(value::int),
        ColumnData::F32(v) => v.map(|x| value::float(x as f64)),
        ColumnData::F64(v) => v.map(value::float),
        ColumnData::Bit(v) => v.map(Value::Bool),
        ColumnData::String(v) => v.as_ref().map(|s| Value::String(s.to_string())),
        ColumnData::Guid(v) => v.map(|g| Value::String(g.to_string().to_uppercase())),
        ColumnData::Binary(v) => v.as_ref().map(|b| value::binary(b)),
        ColumnData::Numeric(v) => v.map(|n| Value::String(n.to_string())),
        ColumnData::Xml(v) => v.as_ref().map(|x| Value::String(x.to_string())),
        ColumnData::DateTime(_) | ColumnData::SmallDateTime(_) | ColumnData::DateTime2(_) => {
            chrono::NaiveDateTime::from_sql(data)
                .ok()
                .flatten()
                .map(|d| Value::String(d.format("%Y-%m-%d %H:%M:%S%.f").to_string()))
        }
        ColumnData::Date(_) => chrono::NaiveDate::from_sql(data)
            .ok()
            .flatten()
            .map(|d| Value::String(d.to_string())),
        ColumnData::Time(_) => chrono::NaiveTime::from_sql(data)
            .ok()
            .flatten()
            .map(|d| Value::String(d.to_string())),
        ColumnData::DateTimeOffset(_) => chrono::DateTime::<chrono::FixedOffset>::from_sql(data)
            .ok()
            .flatten()
            .map(|d| Value::String(d.to_rfc3339())),
    };
    v.unwrap_or(Value::Null)
}

/// Statements whose affected row count matters (sent as RPC to get it back).
fn counts_rows(sql: &str) -> bool {
    matches!(
        leading_keyword(sql).as_str(),
        "INSERT" | "UPDATE" | "DELETE" | "MERGE"
    )
}

impl SqlServerConn {
    async fn query(&mut self, sql: &str, skip: usize, max: usize) -> DbResult<RawResult> {
        let mut out = RawResult::default();
        if counts_rows(sql) {
            let res = self.client.execute(sql, &[]).await?;
            out.rows_affected = res.rows_affected().iter().sum();
            return Ok(out);
        }
        let mut stream = self.client.simple_query(sql).await?;
        let mut seen = 0usize;
        while let Some(item) = stream.try_next().await? {
            match item {
                QueryItem::Metadata(meta) if meta.result_index() == 0 => {
                    out.columns = meta
                        .columns()
                        .iter()
                        .map(|c| ColumnMeta {
                            name: c.name().to_string(),
                            type_name: format!("{:?}", c.column_type()),
                        })
                        .collect();
                }
                QueryItem::Row(row) if row.result_index() == 0 => {
                    seen += 1;
                    if seen <= skip {
                        continue;
                    }
                    if out.rows.len() >= max {
                        out.has_more = true;
                        // The rest is drained before the next query on this connection.
                        break;
                    }
                    out.rows.push(row.into_iter().map(|d| cell(&d)).collect());
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

impl EngineConn for SqlServerConn {
    fn dialect(&self) -> Dialect {
        Dialect::SqlServer
    }

    fn run<'a>(
        &'a mut self,
        sql: &'a str,
        skip: usize,
        max: usize,
    ) -> BoxFuture<'a, DbResult<RawResult>> {
        Box::pin(self.query(sql, skip, max))
    }

    fn server_version(&mut self) -> BoxFuture<'_, DbResult<String>> {
        Box::pin(async move {
            let r = self
                .query(
                    "SELECT CONCAT('SQL Server ', CAST(SERVERPROPERTY('ProductVersion') AS nvarchar(64)), ' ', CAST(SERVERPROPERTY('Edition') AS nvarchar(128)))",
                    0,
                    1,
                )
                .await?;
            Ok(r.rows
                .first()
                .and_then(|r| r.first())
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string())
        })
    }

    // A running TDS request cannot be interrupted from another task: the statement future
    // is dropped and the connection replaced.
    fn reconnect_after_cancel(&self) -> bool {
        true
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DataSource;
    use crate::tls::TlsMaterial;
    use serde_json::json;
    use std::borrow::Cow;

    fn spec(ds: &DataSource) -> ConnectSpec<'_> {
        ConnectSpec {
            ds,
            password: Some("Secret!1"),
            tls: TlsMaterial::default(),
            tls_host: "db.example.com".into(),
            root: None,
        }
    }

    #[test]
    fn maps_data_source_to_tds_config() {
        let mut ds = DataSource {
            driver: crate::config::Driver::Sqlserver,
            host: "127.0.0.1".into(),
            port: Some(14330),
            user: "sa".into(),
            database: "shop".into(),
            ..Default::default()
        };
        assert_eq!(
            config_for(&spec(&ds)).unwrap().get_addr(),
            "127.0.0.1:14330"
        );
        ds.options.ssl_mode = SslMode::VerifyFull;
        assert!(config_for(&spec(&ds)).is_ok());
        ds.options.ssl.cert_file = "client.pem".into();
        assert!(config_for(&spec(&ds))
            .unwrap_err()
            .to_string()
            .contains("both a certificate and a key"));
    }

    #[test]
    fn converts_tds_values() {
        assert_eq!(cell(&ColumnData::I32(Some(42))), json!(42));
        assert_eq!(
            cell(&ColumnData::I64(Some(1 << 60))),
            json!("1152921504606846976")
        );
        assert_eq!(cell(&ColumnData::Bit(Some(true))), json!(true));
        assert_eq!(cell(&ColumnData::String(None)), Value::Null);
        assert_eq!(
            cell(&ColumnData::String(Some(Cow::Owned("Ada".into())))),
            json!("Ada")
        );
        assert_eq!(
            cell(&ColumnData::Binary(Some(Cow::Owned(vec![0xca, 0xfe])))),
            json!({"$hex": "cafe"})
        );
        assert_eq!(
            cell(&ColumnData::Numeric(Some(
                tiberius::numeric::Numeric::new_with_scale(12345, 2)
            ))),
            json!("123.45")
        );
        assert!(counts_rows("update t set a = 1"));
        assert!(!counts_rows("select * from t"));
    }
}
