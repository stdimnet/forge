//! Redis data sources against an in-process RESP fake: the keys tree, the value viewer, the
//! command console, read-only refusal and the production confirmation.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use forge_db::{DataSource, DbError, DbService, Driver, MemoryStore};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Clone, Debug)]
enum Val {
    Str(String),
    List(Vec<String>),
    Hash(Vec<(String, String)>),
    Set(Vec<String>),
    ZSet(Vec<(String, f64)>),
}

type Store = Arc<Mutex<BTreeMap<(i64, String), (Val, i64)>>>;

fn bulk(s: &str) -> String {
    format!("${}\r\n{s}\r\n", s.len())
}

fn array(items: &[String]) -> String {
    format!("*{}\r\n{}", items.len(), items.concat())
}

fn glob(p: &str, s: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], s) || (!s.is_empty() && go(p, &s[1..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    go(p.as_bytes(), s.as_bytes())
}

fn reply(store: &Store, db: &mut i64, cmd: &[String]) -> String {
    let name = cmd[0].to_ascii_uppercase();
    let mut st = store.lock().unwrap();
    let arg = |i: usize| cmd.get(i).cloned().unwrap_or_default();
    let key = (*db, arg(1));
    match name.as_str() {
        "PING" => "+PONG\r\n".into(),
        "CLIENT" | "AUTH" => "+OK\r\n".into(),
        "SELECT" => {
            *db = arg(1).parse().unwrap();
            "+OK\r\n".into()
        }
        "INFO" if arg(1).eq_ignore_ascii_case("server") => {
            bulk("# Server\r\nredis_version:7.2.4\r\n")
        }
        "INFO" => {
            let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
            for (d, _) in st.keys() {
                *counts.entry(*d).or_default() += 1;
            }
            let lines: String = counts
                .iter()
                .map(|(d, n)| format!("db{d}:keys={n},expires=0,avg_ttl=0\r\n"))
                .collect();
            bulk(&format!("# Keyspace\r\n{lines}"))
        }
        "SCAN" => {
            let pattern = cmd
                .iter()
                .position(|a| a.eq_ignore_ascii_case("MATCH"))
                .map(|i| arg(i + 1))
                .unwrap_or("*".into());
            let keys: Vec<String> = st
                .keys()
                .filter(|(d, k)| *d == *db && glob(&pattern, k))
                .map(|(_, k)| bulk(k))
                .collect();
            array(&[bulk("0"), array(&keys)])
        }
        "TYPE" => match st.get(&key) {
            None => "+none\r\n".into(),
            Some((v, _)) => format!(
                "+{}\r\n",
                match v {
                    Val::Str(_) => "string",
                    Val::List(_) => "list",
                    Val::Hash(_) => "hash",
                    Val::Set(_) => "set",
                    Val::ZSet(_) => "zset",
                }
            ),
        },
        "PTTL" => match st.get(&key) {
            None => ":-2\r\n".into(),
            Some((_, ttl)) => format!(":{ttl}\r\n"),
        },
        "GET" => match st.get(&key) {
            Some((Val::Str(s), _)) => bulk(s),
            _ => "$-1\r\n".into(),
        },
        "SET" => {
            st.insert(key, (Val::Str(arg(2)), -1));
            "+OK\r\n".into()
        }
        "DEL" => format!(":{}\r\n", st.remove(&key).is_some() as i32),
        "FLUSHDB" => {
            st.retain(|(d, _), _| *d != *db);
            "+OK\r\n".into()
        }
        "LLEN" | "HLEN" | "SCARD" | "ZCARD" => format!(
            ":{}\r\n",
            match st.get(&key) {
                Some((Val::List(l), _)) | Some((Val::Set(l), _)) => l.len(),
                Some((Val::Hash(h), _)) => h.len(),
                Some((Val::ZSet(z), _)) => z.len(),
                _ => 0,
            }
        ),
        "LRANGE" => match st.get(&key) {
            Some((Val::List(l), _)) => array(&l.iter().map(|x| bulk(x)).collect::<Vec<_>>()),
            _ => "*0\r\n".into(),
        },
        "SSCAN" => match st.get(&key) {
            Some((Val::Set(l), _)) => array(&[
                bulk("0"),
                array(&l.iter().map(|x| bulk(x)).collect::<Vec<_>>()),
            ]),
            _ => array(&[bulk("0"), "*0\r\n".into()]),
        },
        "HGETALL" => match st.get(&key) {
            Some((Val::Hash(h), _)) => array(
                &h.iter()
                    .flat_map(|(k, v)| [bulk(k), bulk(v)])
                    .collect::<Vec<_>>(),
            ),
            _ => "*0\r\n".into(),
        },
        "ZRANGE" => match st.get(&key) {
            Some((Val::ZSet(z), _)) => array(
                &z.iter()
                    .flat_map(|(m, s)| [bulk(m), bulk(&s.to_string())])
                    .collect::<Vec<_>>(),
            ),
            _ => "*0\r\n".into(),
        },
        _ => format!("-ERR unknown command '{name}'\r\n"),
    }
}

async fn read_command<R: AsyncBufReadExt + Unpin>(r: &mut R) -> Option<Vec<String>> {
    let mut line = String::new();
    if r.read_line(&mut line).await.ok()? == 0 {
        return None;
    }
    let n: usize = line.trim().strip_prefix('*')?.parse().ok()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        line.clear();
        r.read_line(&mut line).await.ok()?;
        let len: usize = line.trim().strip_prefix('$')?.parse().ok()?;
        let mut buf = vec![0u8; len + 2];
        r.read_exact(&mut buf).await.ok()?;
        out.push(String::from_utf8_lossy(&buf[..len]).into_owned());
    }
    Some(out)
}

async fn start_fake() -> (u16, Store) {
    let store: Store = Arc::default();
    {
        let mut s = store.lock().unwrap();
        s.insert((0, "greeting".into()), (Val::Str("hello".into()), -1));
        s.insert(
            (0, "session:42".into()),
            (Val::Str("{\"user\":1}".into()), 3_600_000),
        );
        s.insert(
            (0, "queue:mail".into()),
            (Val::List(vec!["a@x".into(), "b@x".into()]), -1),
        );
        s.insert(
            (0, "user:1".into()),
            (
                Val::Hash(vec![
                    ("name".into(), "Ada".into()),
                    ("role".into(), "admin".into()),
                ]),
                -1,
            ),
        );
        s.insert(
            (0, "user:1:tags".into()),
            (Val::Set(vec!["math".into(), "poetry".into()]), -1),
        );
        s.insert(
            (0, "leaderboard".into()),
            (
                Val::ZSet(vec![("ada".into(), 99.5), ("alan".into(), 87.0)]),
                -1,
            ),
        );
        s.insert(
            (1, "cache:home".into()),
            (Val::Str("<html>".into()), 60_000),
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let st = store.clone();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let st = st.clone();
            tokio::spawn(async move {
                let (r, mut w) = socket.into_split();
                let mut r = BufReader::new(r);
                let mut db = 0;
                while let Some(cmd) = read_command(&mut r).await {
                    let out = reply(&st, &mut db, &cmd);
                    if w.write_all(out.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    (port, store)
}

async fn service(port: u16, configure: impl FnOnce(&mut DataSource)) -> (DbService, String) {
    let svc = DbService::new(None, Arc::new(MemoryStore::default()));
    let mut ds = DataSource {
        name: "cache".into(),
        driver: Driver::Redis,
        host: "127.0.0.1".into(),
        port: Some(port),
        scope: forge_db::Scope::Global,
        ..Default::default()
    };
    ds.options.connect_timeout_secs = 2;
    configure(&mut ds);
    let info = svc.save(ds, None).await.unwrap();
    (svc, info.source.id)
}

#[tokio::test]
async fn browses_keys_and_values() {
    let (port, _) = start_fake().await;
    let (svc, id) = service(port, |_| {}).await;
    let ds = svc.get(&id).unwrap();
    let t = svc.test(&ds, None).await;
    assert!(t.ok, "{t:?}");
    assert_eq!(t.server_version.as_deref(), Some("Redis 7.2.4"));
    let info = &svc.list()[0];
    assert_eq!(info.summary, format!("127.0.0.1:{port}/0"));
    assert!(!info.transactions);

    let schemas = svc.schemas(&id).await.unwrap();
    let names: Vec<_> = schemas
        .iter()
        .map(|s| (s.name.as_str(), s.current))
        .collect();
    assert_eq!(names, vec![("db0", true), ("db1", false)]);

    let keys = svc.redis_keys(&id, "db0", "user:*", None).await.unwrap();
    let k: Vec<_> = keys
        .keys
        .iter()
        .map(|k| (k.key.as_str(), k.kind.as_str()))
        .collect();
    assert_eq!(k, vec![("user:1", "hash"), ("user:1:tags", "set")]);
    let all = svc.redis_keys(&id, "db0", "", None).await.unwrap();
    assert_eq!(all.keys.len(), 6);
    let session = all.keys.iter().find(|k| k.key == "session:42").unwrap();
    assert_eq!(session.ttl_ms, 3_600_000);
    let other = svc.redis_keys(&id, "db1", "*", None).await.unwrap();
    assert_eq!(other.keys[0].key, "cache:home");

    let v = svc.redis_value(&id, "db0", "user:1").await.unwrap();
    assert_eq!(v.kind, "hash");
    assert_eq!(v.value, json!({"name": "Ada", "role": "admin"}));
    assert_eq!(v.length, 2);
    let v = svc.redis_value(&id, "db0", "leaderboard").await.unwrap();
    assert_eq!(
        v.value,
        json!([{"member": "ada", "score": 99.5}, {"member": "alan", "score": 87.0}])
    );
    let v = svc.redis_value(&id, "db0", "queue:mail").await.unwrap();
    assert_eq!(v.value, json!(["a@x", "b@x"]));
    let v = svc.redis_value(&id, "db0", "user:1:tags").await.unwrap();
    assert_eq!(v.value, json!(["math", "poetry"]));
    let v = svc.redis_value(&id, "db0", "greeting").await.unwrap();
    assert_eq!((v.value, v.length), (json!("hello"), 5));
    assert!(svc.redis_value(&id, "db0", "missing").await.is_err());
}

#[tokio::test]
async fn runs_console_commands() {
    let (port, store) = start_fake().await;
    let (svc, id) = service(port, |_| {}).await;
    let s = svc.open_session(&id).await.unwrap();
    assert!(s.auto_commit);
    assert!(svc.set_auto_commit(&s.id, false).await.is_err());
    let res = svc
        .execute(
            &s.id,
            "# set and read\nSET motd \"hello world\"\nGET motd\nHGETALL user:1",
            None,
        )
        .await
        .unwrap();
    assert!(res.iter().all(|r| r.error.is_none()), "{res:?}");
    assert_eq!(res[0].result.rows, vec![vec![json!("OK")]]);
    assert_eq!(res[1].result.rows, vec![vec![json!("hello world")]]);
    assert_eq!(res[2].result.columns[0].name, "field");
    assert_eq!(res[2].result.rows.len(), 2);
    assert!(store.lock().unwrap().contains_key(&(0, "motd".into())));
    let res = svc.execute(&s.id, "NOPE x", None).await.unwrap();
    assert!(res[0].error.as_deref().unwrap().contains("unknown command"));
}

#[tokio::test]
async fn read_only_and_production_guards() {
    let (port, store) = start_fake().await;
    let (svc, id) = service(port, |ds| ds.options.read_only = true).await;
    let s = svc.open_session(&id).await.unwrap();
    let err = svc
        .execute(&s.id, "GET greeting\nDEL greeting", None)
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::Refused(_)), "{err}");
    assert!(err.to_string().contains("cache is read-only: DEL"), "{err}");
    assert!(store.lock().unwrap().contains_key(&(0, "greeting".into())));
    let ok = svc.execute(&s.id, "GET greeting", None).await.unwrap();
    assert_eq!(ok[0].result.rows, vec![vec![json!("hello")]]);

    let (svc, id) = service(port, |ds| ds.options.color = Some("red".into())).await;
    assert!(svc.list()[0].production);
    let guard = svc.guard(&id, "FLUSHDB").unwrap();
    assert!(guard.confirm.unwrap().contains("production"));
    let s = svc.open_session(&id).await.unwrap();
    let err = svc.execute(&s.id, "FLUSHDB", None).await.unwrap_err();
    assert!(matches!(err, DbError::NeedsConfirmation(_)));
    assert_eq!(store.lock().unwrap().len(), 7);
    svc.execute_with(&s.id, "FLUSHDB", None, true)
        .await
        .unwrap();
    assert_eq!(store.lock().unwrap().len(), 1);
}
