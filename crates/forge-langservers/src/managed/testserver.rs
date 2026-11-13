//! A tiny HTTP/1.1 server on localhost for tests: fixed bodies per path, request counts,
//! the `Authorization` header each path received, protected paths and redirects.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
pub struct Routes {
    bodies: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    hits: Arc<Mutex<HashMap<String, usize>>>,
    auth: Arc<Mutex<HashMap<String, Option<String>>>>,
    tokens: Arc<Mutex<HashMap<String, String>>>,
    redirects: Arc<Mutex<HashMap<String, String>>>,
}

impl Routes {
    pub fn set(&self, path: &str, body: impl Into<Vec<u8>>) {
        self.bodies
            .lock()
            .unwrap()
            .insert(path.to_string(), body.into());
    }

    pub fn hits(&self, path: &str) -> usize {
        self.hits.lock().unwrap().get(path).copied().unwrap_or(0)
    }

    /// The `Authorization` header of the last request for `path` (`None`: none was sent,
    /// or the path was never requested).
    pub fn authorization(&self, path: &str) -> Option<String> {
        self.auth.lock().unwrap().get(path).cloned().flatten()
    }

    /// Answer 401 for `path` unless the request carries `Authorization: Bearer <token>`.
    pub fn protect(&self, path: &str, token: &str) {
        self.tokens
            .lock()
            .unwrap()
            .insert(path.to_string(), token.to_string());
    }

    /// Answer `path` with a 302 redirect to `location`.
    pub fn redirect(&self, path: &str, location: &str) {
        self.redirects
            .lock()
            .unwrap()
            .insert(path.to_string(), location.to_string());
    }
}

pub struct TestServer {
    pub url: String,
    pub routes: Routes,
}

/// Start a server; unknown paths answer 404.
pub async fn start() -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let routes = Routes::default();
    let r = routes.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let r = r.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&buf);
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let auth = head.lines().skip(1).find_map(|l| {
                    let (name, value) = l.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_string())
                });
                *r.hits.lock().unwrap().entry(path.clone()).or_default() += 1;
                r.auth.lock().unwrap().insert(path.clone(), auth.clone());
                let token = r.tokens.lock().unwrap().get(&path).cloned();
                let redirect = r.redirects.lock().unwrap().get(&path).cloned();
                let body = r.bodies.lock().unwrap().get(&path).cloned();
                let mut extra = String::new();
                let (status, body) = if token.is_some_and(|t| auth != Some(format!("Bearer {t}"))) {
                    ("401 Unauthorized", b"unauthorized".to_vec())
                } else if let Some(location) = redirect {
                    extra = format!("Location: {location}\r\n");
                    ("302 Found", Vec::new())
                } else {
                    match body {
                        Some(b) => ("200 OK", b),
                        None => ("404 Not Found", b"not found".to_vec()),
                    }
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(header.as_bytes()).await;
                let _ = socket.write_all(&body).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    TestServer { url, routes }
}
