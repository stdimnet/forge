//! ClickHouse over its HTTP interface (port 8123, 8443 with TLS): every statement is a POST
//! with the SQL as the body; results come back as `JSONCompact`. Each console session has
//! its own `session_id` (temporary tables and `SET` survive between statements); a running
//! statement is cancelled with `KILL QUERY` by its `query_id`. Read-only data sources send
//! `readonly=2`, so the server refuses writes.

use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use serde::Deserialize;
use serde_json::Value;

use super::{flat_cell, page_rows, Canceller, ConnectSpec, EngineConn, EnginePool};
use crate::config::{Dialect, SslMode};
use crate::conn::{ColumnMeta, RawResult};
use crate::error::{DbError, DbResult};

struct Inner {
    http: reqwest::Client,
    url: String,
    user: String,
    password: String,
    database: String,
    read_only: bool,
    init: String,
}

pub struct ClickHousePool {
    inner: Arc<Inner>,
}

static SEQ: AtomicU64 = AtomicU64::new(1);

fn unique(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos:x}-{}", SEQ.fetch_add(1, Ordering::SeqCst))
}

/// `http://host:8123` or `https://…` by SSL mode (`prefer` stays on plain HTTP: the HTTP
/// interface cannot negotiate TLS).
pub fn base_url(spec: &ConnectSpec<'_>) -> String {
    let scheme = match spec.ds.options.ssl_mode {
        SslMode::Disable | SslMode::Prefer => "http",
        _ => "https",
    };
    let host = spec.host();
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    };
    format!("{scheme}://{host}:{}/", spec.port())
}

impl ClickHousePool {
    pub async fn connect(spec: &ConnectSpec<'_>) -> DbResult<Self> {
        let mut b = reqwest::Client::builder()
            .connect_timeout(spec.timeout())
            .user_agent("Forge");
        match spec.ds.options.ssl_mode {
            SslMode::Require => b = b.danger_accept_invalid_certs(true),
            SslMode::VerifyCa | SslMode::VerifyFull => {
                if let Some(ca) = &spec.tls.ca {
                    let cert = reqwest::Certificate::from_pem(ca)
                        .map_err(|e| DbError::Config(format!("CA certificate: {e}")))?;
                    b = b.add_root_certificate(cert);
                }
            }
            _ => {}
        }
        if let Some(id) = spec.tls.identity_pem() {
            let identity = reqwest::Identity::from_pem(&id)
                .map_err(|e| DbError::Config(format!("client certificate: {e}")))?;
            b = b.identity(identity);
        }
        let pool = Self {
            inner: Arc::new(Inner {
                http: b.build().map_err(|e| DbError::Config(e.to_string()))?,
                url: base_url(spec),
                user: if spec.ds.user.trim().is_empty() {
                    "default".into()
                } else {
                    spec.ds.user.trim().into()
                },
                password: spec.password.unwrap_or_default().into(),
                database: spec.ds.database.trim().into(),
                read_only: spec.ds.options.read_only,
                init: spec.ds.options.init_sql.trim().into(),
            }),
        };
        // Check the server and the login now.
        let mut conn = pool.open();
        conn.query("SELECT 1", 0, 1).await?;
        Ok(pool)
    }

    fn open(&self) -> ClickHouseConn {
        ClickHouseConn {
            inner: self.inner.clone(),
            session: unique("forge"),
            running: Arc::default(),
            initialized: self.inner.init.is_empty(),
        }
    }
}

impl EnginePool for ClickHousePool {
    fn dialect(&self) -> Dialect {
        Dialect::ClickHouse
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn acquire(&self) -> BoxFuture<'_, DbResult<Box<dyn EngineConn>>> {
        Box::pin(async move { Ok(Box::new(self.open()) as Box<dyn EngineConn>) })
    }
}

pub struct ClickHouseConn {
    inner: Arc<Inner>,
    session: String,
    /// `query_id` of the running statement.
    running: Arc<Mutex<Option<String>>>,
    initialized: bool,
}

#[derive(Deserialize)]
struct Meta {
    name: String,
    #[serde(rename = "type")]
    type_name: String,
}

#[derive(Deserialize)]
struct Compact {
    meta: Vec<Meta>,
    data: Vec<Vec<Value>>,
}

/// Parse a response body: `JSONCompact`, empty (no result set) or text of another `FORMAT`.
pub fn parse_body(body: &str, summary: Option<&str>) -> DbResult<RawResult> {
    let mut out = RawResult::default();
    if let Some(s) = summary {
        if let Ok(v) = serde_json::from_str::<Value>(s) {
            out.rows_affected = v
                .get("written_rows")
                .and_then(|w| w.as_str().and_then(|s| s.parse().ok()).or(w.as_u64()))
                .unwrap_or(0);
        }
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(out);
    }
    if trimmed.starts_with('{') {
        if let Ok(c) = serde_json::from_str::<Compact>(trimmed) {
            out.columns = c
                .meta
                .into_iter()
                .map(|m| ColumnMeta {
                    name: m.name,
                    type_name: m.type_name,
                })
                .collect();
            out.rows = c
                .data
                .into_iter()
                .map(|r| r.into_iter().map(flat_cell).collect())
                .collect();
            return Ok(out);
        }
    }
    // `FORMAT CSV`, `FORMAT Pretty`, …: one text line per row.
    out.columns = vec![ColumnMeta {
        name: "result".into(),
        type_name: "String".into(),
    }];
    out.rows = trimmed
        .lines()
        .map(|l| vec![Value::String(l.to_string())])
        .collect();
    Ok(out)
}

/// The message of a ClickHouse error body (`Code: 60. DB::Exception: Table … doesn't exist. (UNKNOWN_TABLE) (version …)`).
pub fn error_message(body: &str) -> String {
    let t = body.trim();
    let t = t.split(" (version ").next().unwrap_or(t);
    t.replace("DB::Exception: ", "").trim().to_string()
}

impl ClickHouseConn {
    async fn post(&self, sql: &str, query_id: &str, session: bool) -> DbResult<RawResult> {
        let i = &self.inner;
        let mut params: Vec<(&str, &str)> = vec![
            ("default_format", "JSONCompact"),
            ("output_format_json_quote_64bit_integers", "1"),
            ("query_id", query_id),
        ];
        if !i.database.is_empty() {
            params.push(("database", &i.database));
        }
        if session {
            params.push(("session_id", &self.session));
        }
        if i.read_only {
            // 2: reads and settings changes only.
            params.push(("readonly", "2"));
        }
        let res = i
            .http
            .post(&i.url)
            .query(&params)
            .header("X-ClickHouse-User", &i.user)
            .header("X-ClickHouse-Key", &i.password)
            .body(sql.to_string())
            .send()
            .await?;
        let status = res.status();
        let summary = res
            .headers()
            .get("X-ClickHouse-Summary")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = res.text().await?;
        if !status.is_success() {
            return Err(DbError::Sql(error_message(&body)));
        }
        parse_body(&body, summary.as_deref())
    }

    async fn query(&mut self, sql: &str, skip: usize, max: usize) -> DbResult<RawResult> {
        if !self.initialized {
            let init = self.inner.init.clone();
            for st in crate::split::split_statements(&init, Dialect::ClickHouse) {
                self.post(&st.text, &unique("forge-init"), true).await?;
            }
            self.initialized = true;
        }
        let id = unique("forge-q");
        *self.running.lock().expect("poisoned") = Some(id.clone());
        let res = self.post(sql, &id, true).await;
        *self.running.lock().expect("poisoned") = None;
        let mut raw = res?;
        let (rows, more) = page_rows(std::mem::take(&mut raw.rows), skip, max);
        raw.rows = rows;
        raw.has_more = more;
        Ok(raw)
    }
}

impl EngineConn for ClickHouseConn {
    fn dialect(&self) -> Dialect {
        Dialect::ClickHouse
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
            let r = self.query("SELECT version()", 0, 1).await?;
            Ok(r.rows
                .first()
                .and_then(|r| r.first())
                .and_then(Value::as_str)
                .map(|v| format!("ClickHouse {v}"))
                .unwrap_or_default())
        })
    }

    fn canceller(&self) -> Option<Canceller> {
        let running = self.running.clone();
        let other = ClickHouseConn {
            inner: self.inner.clone(),
            session: String::new(),
            running: Arc::default(),
            initialized: true,
        };
        let other = Arc::new(other);
        Some(Arc::new(move || {
            let id = running.lock().expect("poisoned").clone();
            let other = other.clone();
            Box::pin(async move {
                if let Some(id) = id {
                    let sql = format!(
                        "KILL QUERY WHERE query_id = {}",
                        Dialect::ClickHouse.quote_str(&id)
                    );
                    if let Err(e) = other.post(&sql, &unique("forge-kill"), false).await {
                        tracing::warn!("cancel failed: {e}");
                    }
                }
            })
        }))
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_json_compact_and_text_results() {
        let body = r#"{"meta":[{"name":"id","type":"UInt64"},{"name":"tags","type":"Array(String)"}],
            "data":[["18446744073709551615",["a","b"]],["2",[]]],"rows":2,"statistics":{"elapsed":0.001}}"#;
        let r = parse_body(body, None).unwrap();
        assert_eq!(r.columns[0].type_name, "UInt64");
        assert_eq!(
            r.rows[0],
            vec![json!("18446744073709551615"), json!("[\"a\",\"b\"]")]
        );
        let r = parse_body("", Some(r#"{"read_rows":"0","written_rows":"3"}"#)).unwrap();
        assert_eq!(r.rows_affected, 3);
        assert!(r.columns.is_empty());
        let r = parse_body("1,\"a\"\n2,\"b\"\n", None).unwrap();
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.columns[0].name, "result");
    }

    #[test]
    fn cleans_error_messages() {
        assert_eq!(
            error_message("Code: 60. DB::Exception: Table default.x does not exist. (UNKNOWN_TABLE) (version 24.3.1.1)\n"),
            "Code: 60. Table default.x does not exist. (UNKNOWN_TABLE)"
        );
    }
}
