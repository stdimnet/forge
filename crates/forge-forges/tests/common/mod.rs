//! A tiny HTTP/1.1 mock server on a local TcpListener for host API tests.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct Route {
    pub method: &'static str,
    /// Path without query; `{base}` in bodies/headers is replaced by the server URL.
    pub path: String,
    /// Substring the query string must contain (to tell pages apart).
    pub query: Option<&'static str>,
    pub status: u16,
    pub content_type: &'static str,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Route {
    pub fn json(method: &'static str, path: &str, body: &str) -> Self {
        Self {
            method,
            path: path.to_string(),
            query: None,
            status: 200,
            content_type: "application/json",
            headers: Vec::new(),
            body: body.to_string(),
        }
    }

    pub fn text(method: &'static str, path: &str, body: &str) -> Self {
        Self {
            content_type: "text/plain",
            ..Self::json(method, path, body)
        }
    }

    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    pub fn query(mut self, q: &'static str) -> Self {
        self.query = Some(q);
        self
    }

    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }
}

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl Recorded {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }
}

pub struct MockServer {
    pub base: String,
    pub requests: Arc<Mutex<Vec<Recorded>>>,
}

impl MockServer {
    pub async fn start(routes: Vec<Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let routes = Arc::new(routes);
        let server_base = base.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                let recorded = recorded.clone();
                let base = server_base.clone();
                tokio::spawn(async move {
                    let Some(req) = read_request(&mut sock).await else {
                        return;
                    };
                    recorded.lock().unwrap().push(req.clone());
                    // Later routes win so tests can override defaults.
                    let route = routes.iter().rev().find(|r| {
                        r.method == req.method
                            && r.path == req.path
                            && r.query.map_or(true, |q| req.query.contains(q))
                    });
                    let Some(route) = route else {
                        let _ = sock
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: 23\r\nConnection: close\r\n\r\n{\"message\":\"Not Found\"}")
                            .await;
                        return;
                    };
                    let body = route.body.replace("{base}", &base);
                    let mut head = format!(
                        "HTTP/1.1 {} Mock\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                        route.status,
                        route.content_type,
                        body.len()
                    );
                    for (k, v) in &route.headers {
                        head.push_str(&format!("{k}: {}\r\n", v.replace("{base}", &base)));
                    }
                    head.push_str("\r\n");
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(body.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        Self { base, requests }
    }

    pub fn find(&self, method: &str, path: &str) -> Recorded {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|r| r.method == method && r.path == path)
            .cloned()
            .unwrap_or_else(|| panic!("no {method} {path} request"))
    }

    pub fn count(&self, method: &str, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.method == method && r.path == path)
            .count()
    }
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<Recorded> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?.to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[header_end..].to_vec();
    while body.len() < len {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    Some(Recorded {
        method,
        path,
        query,
        headers,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}
