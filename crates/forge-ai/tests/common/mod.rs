//! A tiny HTTP/1.1 mock server on a local TcpListener. Responses are written in
//! configurable chunks with pauses so streaming parsers see arbitrary split points.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
    pub status: u16,
    pub content_type: &'static str,
    pub chunks: Vec<Vec<u8>>,
    pub delay: Duration,
}

impl Route {
    pub fn json(method: &'static str, path: &'static str, status: u16, body: &str) -> Self {
        Self {
            method,
            path,
            status,
            content_type: "application/json",
            chunks: vec![body.as_bytes().to_vec()],
            delay: Duration::ZERO,
        }
    }

    /// An SSE response split into `chunk_size`-byte pieces.
    pub fn sse(path: &'static str, body: &str, chunk_size: usize) -> Self {
        Self {
            method: "POST",
            path,
            status: 200,
            content_type: "text/event-stream",
            chunks: body
                .as_bytes()
                .chunks(chunk_size)
                .map(<[u8]>::to_vec)
                .collect(),
            delay: Duration::from_millis(1),
        }
    }

    pub fn with_delay(mut self, d: Duration) -> Self {
        self.delay = d;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
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
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                let recorded = recorded.clone();
                tokio::spawn(async move {
                    let Some(req) = read_request(&mut sock).await else {
                        return;
                    };
                    let path_only = req.path.split('?').next().unwrap_or_default().to_string();
                    recorded.lock().unwrap().push(req.clone());
                    let route = routes
                        .iter()
                        .find(|r| r.method == req.method && format!("/v1{}", r.path) == path_only);
                    let Some(route) = route else {
                        let _ = sock
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                        return;
                    };
                    let head = format!(
                        "HTTP/1.1 {} Mock\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
                        route.status, route.content_type
                    );
                    if sock.write_all(head.as_bytes()).await.is_err() {
                        return;
                    }
                    for chunk in &route.chunks {
                        if sock.write_all(chunk).await.is_err() || sock.flush().await.is_err() {
                            return;
                        }
                        if !route.delay.is_zero() {
                            tokio::time::sleep(route.delay).await;
                        }
                    }
                    let _ = sock.shutdown().await;
                });
            }
        });
        Self { base, requests }
    }

    pub fn last(&self) -> Recorded {
        self.requests
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a request")
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
    let path = first.next()?.to_string();
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
        headers,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

/// A localhost URL nothing listens on.
pub async fn dead_base_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}/v1")
}
