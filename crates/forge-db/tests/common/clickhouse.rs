//! A ClickHouse HTTP interface mock: answers the statements Forge sends with `JSONCompact`
//! bodies and records every request.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn param(&self, name: &str) -> Option<String> {
        self.query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == name).then(|| v.replace("%27", "'").replace('+', " "))
        })
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub type Requests = Arc<Mutex<Vec<Request>>>;

fn compact(meta: &[(&str, &str)], data: serde_json::Value) -> String {
    let meta: Vec<_> = meta
        .iter()
        .map(|(n, t)| serde_json::json!({ "name": n, "type": t }))
        .collect();
    serde_json::json!({ "meta": meta, "data": data, "rows": 0 }).to_string()
}

fn respond(req: &Request) -> (u16, Vec<(String, String)>, String) {
    use serde_json::json;
    if req.header("X-ClickHouse-Key") != Some("secret") {
        return (516, vec![], "Code: 516. DB::Exception: default: Authentication failed. (AUTHENTICATION_FAILED) (version 24.3.1.1)".into());
    }
    let sql = req.body.trim();
    let ok = |body: String| (200, vec![], body);
    if sql.starts_with("INSERT") {
        if req.param("readonly").is_some() {
            return (500, vec![], "Code: 164. DB::Exception: Cannot execute query in readonly mode. (READONLY) (version 24.3.1.1)".into());
        }
        return (
            200,
            vec![(
                "X-ClickHouse-Summary".into(),
                r#"{"read_rows":"0","written_rows":"2"}"#.into(),
            )],
            String::new(),
        );
    }
    match sql {
        "SELECT 1" => ok(compact(&[("1", "UInt8")], json!([[1]]))),
        "SELECT version()" => ok(compact(&[("version()", "String")], json!([["24.3.1.1"]]))),
        "SELECT currentDatabase()" => ok(compact(&[("currentDatabase()", "String")], json!([["shop"]]))),
        s if s.starts_with("SELECT name FROM system.databases") => ok(compact(&[("name", "String")], json!([["default"], ["shop"], ["system"]]))),
        s if s.starts_with("SELECT name, engine, comment, total_rows FROM system.tables") => ok(compact(
            &[("name", "String"), ("engine", "String"), ("comment", "String"), ("total_rows", "Nullable(UInt64)")],
            json!([["daily_sales", "View", "", null], ["events", "MergeTree", "Click stream", "1200"]]),
        )),
        s if s.starts_with("SELECT table, name, type") => ok(compact(
            &[("table", "String"), ("name", "String"), ("type", "String"), ("default_kind", "String"), ("default_expression", "String"), ("comment", "String"), ("is_in_primary_key", "UInt8")],
            json!([
                ["events", "id", "UInt64", "", "", "", 1],
                ["events", "at", "DateTime", "DEFAULT", "now()", "", 0],
                ["events", "user", "Nullable(String)", "", "", "who", 0],
                ["daily_sales", "day", "Date", "", "", "", 0]
            ]),
        )),
        s if s.starts_with("SELECT * FROM `shop`.`events`") => ok(compact(
            &[("id", "UInt64"), ("at", "DateTime"), ("user", "Nullable(String)")],
            json!([["1", "2025-01-06 09:30:00", "ada"], ["2", "2025-01-06 09:31:00", null]]),
        )),
        s if s.starts_with("SELECT COUNT(*)") => ok(compact(&[("count()", "UInt64")], json!([["2"]]))),
        s if s.starts_with("SELECT sleep") => {
            std::thread::sleep(std::time::Duration::from_millis(50));
            ok(compact(&[("x", "UInt8")], json!([[0]])))
        }
        _ => (404, vec![], format!("Code: 62. DB::Exception: Syntax error: failed at position 1 ('{sql}'). (SYNTAX_ERROR) (version 24.3.1.1)")),
    }
}

async fn handle(socket: tokio::net::TcpStream, requests: Requests) {
    let (r, mut w) = socket.into_split();
    let mut r = BufReader::new(r);
    let mut line = String::new();
    if r.read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }
    let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let query = target
        .split_once('?')
        .map(|x| x.1.to_string())
        .unwrap_or_default();
    let mut headers = Vec::new();
    let mut len = 0usize;
    loop {
        line.clear();
        if r.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut body = vec![0u8; len];
    let _ = r.read_exact(&mut body).await;
    let req = Request {
        query,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let (status, extra, text) = respond(&req);
    requests.lock().unwrap().push(req);
    let mut head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        text.len()
    );
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = w.write_all(head.as_bytes()).await;
    let _ = w.write_all(text.as_bytes()).await;
    let _ = w.shutdown().await;
}

/// Start the mock; returns its port and the recorded requests.
pub async fn start() -> (u16, Requests) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests: Requests = Arc::default();
    let rec = requests.clone();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(handle(socket, rec.clone()));
        }
    });
    (port, requests)
}
