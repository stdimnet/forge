//! Remote schemas (`$schema` URLs, user mappings with a URL, `$ref`s to other servers) and
//! the SchemaStore catalog, downloaded only when the user allows remote schemas
//! (`jsonSchema.remote.enabled`). Downloads are cached in the user cache dir
//! (`<cache>/forge/json-schemas/<sha256 of the URL>.json`), never in the project, and
//! refreshed after a day; without network the cached copy keeps working.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

/// SchemaStore's catalog of schemas with their file patterns.
pub const CATALOG_URL: &str = "https://www.schemastore.org/api/json/catalog.json";

/// Larger downloads are refused (the biggest SchemaStore schemas are a few MB).
const MAX_SIZE: usize = 16 << 20;

pub struct RemoteCache {
    dir: PathBuf,
    client: reqwest::Client,
    max_age: Duration,
}

impl RemoteCache {
    /// A cache in `dir`, downloading through the system proxy settings.
    pub fn new(dir: PathBuf) -> Result<RemoteCache, String> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(RemoteCache {
            dir,
            client,
            max_age: Duration::from_secs(24 * 60 * 60),
        })
    }

    /// A cache that never uses a proxy (tests talk to a server on localhost).
    pub fn direct(dir: PathBuf, max_age: Duration) -> Result<RemoteCache, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| e.to_string())?;
        Ok(RemoteCache {
            dir,
            client,
            max_age,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path_for(&self, url: &str) -> PathBuf {
        let hash = Sha256::digest(crate::uri::document(url).as_bytes());
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        self.dir.join(format!("{hex}.json"))
    }

    /// The cached copy of `url`, however old.
    pub fn cached(&self, url: &str) -> Option<String> {
        std::fs::read_to_string(self.path_for(url)).ok()
    }

    fn fresh(&self, path: &Path) -> bool {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < self.max_age)
    }

    /// The text of `url`: the cache while it is fresh, else a download (falling back to an
    /// old copy when the download fails).
    pub async fn fetch(&self, url: &str) -> Result<String, String> {
        if !crate::uri::is_remote(url) {
            return Err(format!("not a remote schema: {url}"));
        }
        let path = self.path_for(url);
        if self.fresh(&path) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                return Ok(text);
            }
        }
        match self.download(url).await {
            Ok(text) => {
                self.store(&path, &text);
                Ok(text)
            }
            Err(e) => self.cached(url).ok_or(e),
        }
    }

    async fn download(&self, url: &str) -> Result<String, String> {
        let response = self
            .client
            .get(crate::uri::document(url))
            .send()
            .await
            .map_err(|e| format!("cannot download {url}: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("{url}: HTTP {}", response.status()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("download of {url} failed: {e}"))?;
        if bytes.len() > MAX_SIZE {
            return Err(format!("{url} is too large for a schema"));
        }
        let text = String::from_utf8(bytes.to_vec()).map_err(|_| format!("{url} is not UTF-8"))?;
        if crate::json::parse(&text)
            .and_then(|n| n.as_object().map(|_| ()))
            .is_none()
        {
            return Err(format!("{url} is not a JSON schema"));
        }
        Ok(text)
    }

    fn store(&self, path: &Path, text: &str) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let tmp = path.with_extension("part");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// A tiny HTTP/1.1 server on localhost: fixed bodies per path, request counts.
    async fn server(bodies: HashMap<&'static str, &'static str>) -> (String, Arc<Mutex<usize>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(Mutex::new(0));
        let h = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let bodies = bodies.clone();
                let h = h.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 2048];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    *h.lock().unwrap() += 1;
                    let head = String::from_utf8_lossy(&buf).to_string();
                    let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let (status, body) = match bodies.get(path.as_str()) {
                        Some(b) => ("200 OK", b.to_string()),
                        None => ("404 Not Found", "not found".to_string()),
                    };
                    let reply = format!(
                        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(reply.as_bytes()).await;
                });
            }
        });
        (url, hits)
    }

    #[tokio::test]
    async fn downloads_once_and_caches_in_the_given_dir() {
        let (url, hits) = server(HashMap::from([
            ("/s.json", r#"{"type": "object"}"#),
            ("/bad.json", "<html>"),
        ]))
        .await;
        let dir = tempfile::tempdir().unwrap();
        let cache = RemoteCache::direct(dir.path().join("json-schemas"), Duration::from_secs(3600))
            .unwrap();
        let s = format!("{url}/s.json");
        assert_eq!(cache.fetch(&s).await.unwrap(), r#"{"type": "object"}"#);
        assert!(cache.path_for(&s).starts_with(dir.path()));
        assert_eq!(
            cache.fetch(&format!("{s}#/x")).await.unwrap(),
            r#"{"type": "object"}"#
        );
        assert_eq!(
            *hits.lock().unwrap(),
            1,
            "the second fetch comes from the cache"
        );
        assert!(cache
            .fetch(&format!("{url}/bad.json"))
            .await
            .unwrap_err()
            .contains("not a JSON schema"));
        assert!(cache
            .fetch(&format!("{url}/missing.json"))
            .await
            .unwrap_err()
            .contains("404"));
        assert!(cache.fetch("project:/x.json").await.is_err());
    }

    #[tokio::test]
    async fn stale_copies_are_used_when_offline() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RemoteCache::direct(dir.path().to_path_buf(), Duration::ZERO).unwrap();
        // Nothing listens on this port any more.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/s.json", listener.local_addr().unwrap());
        drop(listener);
        assert!(cache.fetch(&url).await.is_err());
        std::fs::write(cache.path_for(&url), r#"{"title": "old"}"#).unwrap();
        assert_eq!(cache.fetch(&url).await.unwrap(), r#"{"title": "old"}"#);
    }
}
