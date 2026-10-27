//! Redis with `redis-rs` (BSD-3-Clause): databases from `INFO keyspace`, a keys tree from
//! `SCAN` with each key's type and TTL, a value viewer for strings, lists, hashes, sets,
//! sorted sets and streams, and a console that runs one command per line. Read-only data
//! sources refuse every command that is not known to only read.

use std::any::Any;
use std::time::Duration;

use futures_util::future::BoxFuture;
use redis::aio::MultiplexedConnection;
use redis::{
    AsyncConnectionConfig, Client, ConnectionAddr, IntoConnectionInfo, RedisConnectionInfo,
    Value as RValue,
};
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::{shell_words, ConnectSpec, EngineConn, EnginePool};
use crate::config::{Dialect, SslMode};
use crate::conn::{ColumnMeta, RawResult};
use crate::error::{DbError, DbResult};
use crate::introspect::{SchemaInfo, SchemaName};
use crate::value;

/// Elements shown of a big list, hash, set or sorted set.
pub const VALUE_LIMIT: usize = 1000;

pub struct RedisPool {
    addr: ConnectionAddr,
    user: String,
    password: String,
    db: i64,
    tls: Option<redis::TlsCertificates>,
    timeout: Duration,
    read_only: bool,
}

/// A key of the keys tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKey {
    pub key: String,
    /// `string`, `list`, `hash`, `set`, `zset`, `stream`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Milliseconds to live; `-1` without expiry.
    pub ttl_ms: i64,
}

/// Keys matching a pattern.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeys {
    pub db: i64,
    pub pattern: String,
    pub keys: Vec<RedisKey>,
    /// More keys match than were returned.
    pub truncated: bool,
}

/// The value viewer's content.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisValue {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub ttl_ms: i64,
    /// Number of elements (string length for strings).
    pub length: u64,
    /// String: text; list/set: array; hash: object; zset: `[{member, score}]`; stream:
    /// `[{id, fields}]`.
    pub value: Value,
    pub truncated: bool,
}

impl RedisPool {
    pub async fn connect(spec: &ConnectSpec<'_>) -> DbResult<Self> {
        let host = spec.host();
        let port = spec.port();
        let tls = spec.ds.options.ssl_mode != SslMode::Disable
            && spec.ds.options.ssl_mode != SslMode::Prefer;
        let addr = if tls {
            ConnectionAddr::TcpTls {
                host: if spec.ds.options.ssl_mode == SslMode::VerifyFull {
                    spec.tls_host.clone()
                } else {
                    host.clone()
                },
                port,
                insecure: spec.ds.options.ssl_mode == SslMode::Require,
                tls_params: None,
            }
        } else {
            ConnectionAddr::Tcp(host, port)
        };
        let certs =
            (tls && (spec.tls.ca.is_some() || spec.tls.identity_pem().is_some())).then(|| {
                redis::TlsCertificates {
                    client_tls: match (&spec.tls.cert, &spec.tls.key) {
                        (Some(c), Some(k)) => Some(redis::ClientTlsConfig {
                            client_cert: c.clone(),
                            client_key: k.clone(),
                        }),
                        _ => None,
                    },
                    root_cert: spec.tls.ca.clone(),
                }
            });
        let db = match spec.ds.database.trim() {
            "" => 0,
            d => d.trim_start_matches("db").parse().map_err(|_| {
                DbError::Config(format!("Redis database must be a number, not {d}"))
            })?,
        };
        let pool = Self {
            addr,
            user: spec.ds.user.trim().into(),
            password: spec.password.unwrap_or_default().into(),
            db,
            tls: certs,
            timeout: spec.timeout(),
            read_only: spec.ds.options.read_only,
        };
        let mut conn = pool.open(db).await?;
        conn.command(&["PING".into()]).await?;
        Ok(pool)
    }

    fn client(&self, db: i64) -> DbResult<Client> {
        let mut redis = RedisConnectionInfo::default().set_db(db);
        if !self.user.is_empty() {
            redis = redis.set_username(&self.user);
        }
        if !self.password.is_empty() {
            redis = redis.set_password(&self.password);
        }
        let info = self
            .addr
            .clone()
            .into_connection_info()?
            .set_redis_settings(redis);
        Ok(match &self.tls {
            Some(certs) => Client::build_with_tls(info, certs.clone())?,
            None => Client::open(info)?,
        })
    }

    /// A connection to database `db`.
    pub async fn open(&self, db: i64) -> DbResult<RedisConn> {
        let config = AsyncConnectionConfig::new()
            .set_connection_timeout(Some(self.timeout))
            .set_response_timeout(Some(Duration::from_secs(120)));
        let conn = self
            .client(db)?
            .get_multiplexed_async_connection_with_config(&config)
            .await?;
        Ok(RedisConn {
            conn,
            db,
            read_only: self.read_only,
        })
    }

    pub fn default_db(&self) -> i64 {
        self.db
    }
}

impl EnginePool for RedisPool {
    fn dialect(&self) -> Dialect {
        Dialect::Redis
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn acquire(&self) -> BoxFuture<'_, DbResult<Box<dyn EngineConn>>> {
        Box::pin(async move { Ok(Box::new(self.open(self.db).await?) as Box<dyn EngineConn>) })
    }
}

pub struct RedisConn {
    conn: MultiplexedConnection,
    db: i64,
    read_only: bool,
}

fn text(v: &RValue) -> Option<String> {
    match v {
        RValue::BulkString(b) => Some(String::from_utf8_lossy(b).into_owned()),
        RValue::SimpleString(s) => Some(s.clone()),
        RValue::VerbatimString { text, .. } => Some(text.clone()),
        RValue::Int(i) => Some(i.to_string()),
        RValue::Okay => Some("OK".into()),
        RValue::Double(d) => Some(d.to_string()),
        _ => None,
    }
}

/// A reply as JSON: strings (binary as `{ $hex }`), numbers, arrays and objects.
pub fn to_json(v: &RValue) -> Value {
    match v {
        RValue::Nil => Value::Null,
        RValue::Int(i) => value::int(*i),
        RValue::BulkString(b) => match std::str::from_utf8(b) {
            Ok(s) => Value::String(s.into()),
            Err(_) => value::binary(b),
        },
        RValue::SimpleString(s) => Value::String(s.clone()),
        RValue::VerbatimString { text, .. } => Value::String(text.clone()),
        RValue::Okay => Value::String("OK".into()),
        RValue::Double(d) => value::float(*d),
        RValue::Boolean(b) => Value::Bool(*b),
        RValue::Array(items) | RValue::Set(items) => {
            Value::Array(items.iter().map(to_json).collect())
        }
        RValue::Map(pairs) => Value::Object(
            pairs
                .iter()
                .map(|(k, v)| {
                    (
                        text(k).unwrap_or_else(|| to_json(k).to_string()),
                        to_json(v),
                    )
                })
                .collect(),
        ),
        RValue::Attribute { data, .. } => to_json(data),
        RValue::ServerError(e) => Value::String(format!("(error) {e:?}")),
        other => Value::String(format!("{other:?}")),
    }
}

fn col(name: &str) -> ColumnMeta {
    ColumnMeta {
        name: name.into(),
        type_name: "redis".into(),
    }
}

/// Console output of a reply: one cell, a numbered list or field/value pairs (`HGETALL`,
/// `CONFIG GET`, RESP3 maps).
pub fn reply_result(command: &str, reply: &RValue) -> RawResult {
    let cell = |v: &RValue| super::flat_cell(to_json(v));
    let pairs = matches!(
        command.to_ascii_uppercase().as_str(),
        "HGETALL" | "CONFIG" | "XINFO" | "HELLO"
    );
    let mut out = RawResult::default();
    match reply {
        RValue::Map(m) => {
            out.columns = vec![col("field"), col("value")];
            out.rows = m.iter().map(|(k, v)| vec![cell(k), cell(v)]).collect();
        }
        RValue::Array(items) | RValue::Set(items) if pairs && items.len() % 2 == 0 => {
            out.columns = vec![col("field"), col("value")];
            out.rows = items
                .chunks(2)
                .map(|p| vec![cell(&p[0]), cell(&p[1])])
                .collect();
        }
        RValue::Array(items) | RValue::Set(items) => {
            out.columns = vec![col("#"), col("value")];
            out.rows = items
                .iter()
                .enumerate()
                .map(|(i, v)| vec![json!(i + 1), cell(v)])
                .collect();
        }
        other => {
            out.columns = vec![col("value")];
            out.rows = vec![vec![cell(other)]];
        }
    }
    out
}

/// Database numbers and key counts of `INFO keyspace` (`db0:keys=12,expires=1,avg_ttl=0`).
pub fn parse_keyspace(info: &str) -> Vec<(i64, u64)> {
    info.lines()
        .filter_map(|l| {
            let (db, rest) = l.trim().split_once(':')?;
            let n: i64 = db.strip_prefix("db")?.parse().ok()?;
            let keys = rest
                .split(',')
                .find_map(|kv| kv.strip_prefix("keys="))
                .and_then(|k| k.parse().ok())
                .unwrap_or(0);
            Some((n, keys))
        })
        .collect()
}

impl RedisConn {
    /// Run a command given as words.
    pub async fn command(&mut self, words: &[String]) -> DbResult<RValue> {
        let Some((name, args)) = words.split_first() else {
            return Err(DbError::Sql("empty command".into()));
        };
        let mut cmd = redis::cmd(name);
        for a in args {
            cmd.arg(a.as_str());
        }
        Ok(cmd.query_async::<RValue>(&mut self.conn).await?)
    }

    async fn console(&mut self, line: &str, skip: usize, max: usize) -> DbResult<RawResult> {
        if self.read_only && forge_sql::guard::redis_is_write(line) {
            return Err(DbError::Refused(format!(
                "the data source is read-only: {} is not allowed",
                line.split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase()
            )));
        }
        let words = shell_words(line)?;
        if words
            .first()
            .is_some_and(|w| w.eq_ignore_ascii_case("SELECT"))
        {
            if let Some(n) = words.get(1).and_then(|n| n.parse().ok()) {
                self.db = n;
            }
        }
        let reply = self.command(&words).await?;
        let mut out = reply_result(&words[0], &reply);
        let (rows, more) = super::page_rows(std::mem::take(&mut out.rows), skip, max);
        out.rows = rows;
        out.has_more = more;
        Ok(out)
    }

    /// Keys matching `pattern` (`*` for all), at most `limit`, with type and TTL.
    pub async fn keys(&mut self, pattern: &str, limit: usize) -> DbResult<RedisKeys> {
        let pattern = if pattern.trim().is_empty() {
            "*"
        } else {
            pattern.trim()
        };
        let mut cursor = 0u64;
        let mut names: Vec<String> = Vec::new();
        let mut truncated = false;
        loop {
            let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(500)
                .query_async(&mut self.conn)
                .await?;
            names.extend(batch);
            cursor = next;
            if names.len() >= limit {
                truncated = cursor != 0 || names.len() > limit;
                names.truncate(limit);
                break;
            }
            if cursor == 0 {
                break;
            }
        }
        names.sort();
        names.dedup();
        let mut pipe = redis::pipe();
        for n in &names {
            pipe.cmd("TYPE").arg(n).cmd("PTTL").arg(n);
        }
        let meta: Vec<RValue> = if names.is_empty() {
            Vec::new()
        } else {
            pipe.query_async(&mut self.conn).await?
        };
        let keys = names
            .into_iter()
            .zip(meta.chunks(2))
            .map(|(key, m)| RedisKey {
                key,
                kind: text(&m[0]).unwrap_or_default(),
                ttl_ms: match m.get(1) {
                    Some(RValue::Int(t)) if *t >= 0 => *t,
                    _ => -1,
                },
            })
            .collect();
        Ok(RedisKeys {
            db: self.db,
            pattern: pattern.into(),
            keys,
            truncated,
        })
    }

    /// The value of a key for the value viewer.
    pub async fn value(&mut self, key: &str) -> DbResult<RedisValue> {
        let (kind, ttl): (String, i64) = redis::pipe()
            .cmd("TYPE")
            .arg(key)
            .cmd("PTTL")
            .arg(key)
            .query_async(&mut self.conn)
            .await?;
        let last = (VALUE_LIMIT - 1) as i64;
        let q = |name: &str| {
            let mut c = redis::cmd(name);
            c.arg(key);
            c
        };
        let (length, value) = match kind.as_str() {
            "none" => return Err(DbError::Sql(format!("key {key} does not exist"))),
            "string" => {
                let v: RValue = q("GET").query_async(&mut self.conn).await?;
                let j = to_json(&v);
                let len = match &v {
                    RValue::BulkString(b) => b.len() as u64,
                    _ => 0,
                };
                (len, j)
            }
            "list" => {
                let len: u64 = q("LLEN").query_async(&mut self.conn).await?;
                let v: RValue = q("LRANGE")
                    .arg(0)
                    .arg(last)
                    .query_async(&mut self.conn)
                    .await?;
                (len, to_json(&v))
            }
            "set" => {
                let len: u64 = q("SCARD").query_async(&mut self.conn).await?;
                let (_, members): (u64, RValue) = q("SSCAN")
                    .arg(0)
                    .arg("COUNT")
                    .arg(VALUE_LIMIT)
                    .query_async(&mut self.conn)
                    .await?;
                let mut items = match to_json(&members) {
                    Value::Array(a) => a,
                    other => vec![other],
                };
                items.sort_by_key(|v| v.to_string());
                (len, Value::Array(items))
            }
            "hash" => {
                let len: u64 = q("HLEN").query_async(&mut self.conn).await?;
                let v: RValue = q("HGETALL").query_async(&mut self.conn).await?;
                let obj = match &v {
                    RValue::Array(items) => Value::Object(
                        items
                            .chunks(2)
                            .take(VALUE_LIMIT)
                            .filter_map(|p| Some((text(&p[0])?, to_json(p.get(1)?))))
                            .collect::<Map<_, _>>(),
                    ),
                    other => to_json(other),
                };
                (len, obj)
            }
            "zset" => {
                let len: u64 = q("ZCARD").query_async(&mut self.conn).await?;
                let v: RValue = q("ZRANGE")
                    .arg(0)
                    .arg(last)
                    .arg("WITHSCORES")
                    .query_async(&mut self.conn)
                    .await?;
                let items = match &v {
                    RValue::Array(items) => items
                        .chunks(2)
                        .map(|p| {
                            let score = text(&p[1])
                                .and_then(|s| s.parse::<f64>().ok())
                                .map(value::float)
                                .unwrap_or(Value::Null);
                            json!({ "member": to_json(&p[0]), "score": score })
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                (len, Value::Array(items))
            }
            "stream" => {
                let len: u64 = q("XLEN").query_async(&mut self.conn).await?;
                let v: RValue = q("XRANGE")
                    .arg("-")
                    .arg("+")
                    .arg("COUNT")
                    .arg(VALUE_LIMIT)
                    .query_async(&mut self.conn)
                    .await?;
                let items = match &v {
                    RValue::Array(entries) => entries
                        .iter()
                        .filter_map(|e| match e {
                            RValue::Array(parts) if parts.len() == 2 => {
                                let fields = match &parts[1] {
                                    RValue::Array(f) => Value::Object(
                                        f.chunks(2)
                                            .filter_map(|p| {
                                                Some((text(&p[0])?, to_json(p.get(1)?)))
                                            })
                                            .collect(),
                                    ),
                                    other => to_json(other),
                                };
                                Some(json!({ "id": text(&parts[0]), "fields": fields }))
                            }
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                (len, Value::Array(items))
            }
            other => (0, Value::String(format!("<{other} values are not shown>"))),
        };
        let shown = match &value {
            Value::Array(a) => a.len() as u64,
            Value::Object(o) => o.len() as u64,
            _ => length,
        };
        Ok(RedisValue {
            key: key.into(),
            kind,
            ttl_ms: if ttl >= 0 { ttl } else { -1 },
            length,
            truncated: shown < length && kind_is_collection(&value),
            value,
        })
    }
}

fn kind_is_collection(v: &Value) -> bool {
    matches!(v, Value::Array(_) | Value::Object(_))
}

impl EngineConn for RedisConn {
    fn dialect(&self) -> Dialect {
        Dialect::Redis
    }

    fn run<'a>(
        &'a mut self,
        sql: &'a str,
        skip: usize,
        max: usize,
    ) -> BoxFuture<'a, DbResult<RawResult>> {
        Box::pin(self.console(sql, skip, max))
    }

    fn server_version(&mut self) -> BoxFuture<'_, DbResult<String>> {
        Box::pin(async move {
            let info = self.command(&["INFO".into(), "server".into()]).await?;
            let info = text(&info).unwrap_or_default();
            let v = info
                .lines()
                .find_map(|l| l.strip_prefix("redis_version:"))
                .or_else(|| info.lines().find_map(|l| l.strip_prefix("valkey_version:")))
                .unwrap_or("?");
            Ok(format!("Redis {}", v.trim()))
        })
    }

    fn schemas(&mut self) -> BoxFuture<'_, DbResult<Vec<SchemaName>>> {
        Box::pin(async move {
            let info = self.command(&["INFO".into(), "keyspace".into()]).await?;
            let mut dbs = parse_keyspace(&text(&info).unwrap_or_default());
            if !dbs.iter().any(|(n, _)| *n == self.db) {
                dbs.push((self.db, 0));
            }
            dbs.sort();
            Ok(dbs
                .into_iter()
                .map(|(n, _)| SchemaName {
                    name: format!("db{n}"),
                    current: n == self.db,
                })
                .collect())
        })
    }

    fn schema<'a>(
        &'a mut self,
        schema: &'a str,
        _only: Option<&'a str>,
    ) -> BoxFuture<'a, DbResult<SchemaInfo>> {
        // The keys tree is loaded with `DbService::redis_keys`.
        Box::pin(async move {
            Ok(SchemaInfo {
                name: schema.into(),
                tables: Vec::new(),
                routines: Vec::new(),
            })
        })
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `db3` → 3.
pub fn db_number(schema: &str) -> DbResult<i64> {
    schema
        .trim()
        .trim_start_matches("db")
        .parse()
        .map_err(|_| DbError::Config(format!("not a Redis database: {schema}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keyspace_info() {
        let info =
            "# Keyspace\r\ndb0:keys=12,expires=1,avg_ttl=0\r\ndb3:keys=2,expires=0,avg_ttl=0\r\n";
        assert_eq!(parse_keyspace(info), vec![(0, 12), (3, 2)]);
        assert_eq!(db_number("db3").unwrap(), 3);
    }

    #[test]
    fn shapes_replies() {
        let r = reply_result("GET", &RValue::BulkString(b"Ada".to_vec()));
        assert_eq!(r.rows, vec![vec![json!("Ada")]]);
        let r = reply_result(
            "HGETALL",
            &RValue::Array(vec![
                RValue::BulkString(b"name".to_vec()),
                RValue::BulkString(b"Ada".to_vec()),
            ]),
        );
        assert_eq!(r.columns[0].name, "field");
        assert_eq!(r.rows, vec![vec![json!("name"), json!("Ada")]]);
        let r = reply_result("LRANGE", &RValue::Array(vec![RValue::Int(1), RValue::Nil]));
        assert_eq!(
            r.rows,
            vec![vec![json!(1), json!(1)], vec![json!(2), Value::Null]]
        );
        assert_eq!(
            to_json(&RValue::BulkString(vec![0xff, 0x00])),
            json!({"$hex": "ff00"})
        );
    }
}
