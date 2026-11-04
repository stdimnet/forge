//! "Refresh versions" (feature `net`): read the Minecraft versions Paper publishes its API
//! for from `maven-metadata.xml` in Paper's repository. Only runs when the user asks for it.

use std::time::Duration;

use crate::versions::{self, mc_versions_from_paper_metadata, PAPER_METADATA_PATH};

const MAX_SIZE: usize = 4 << 20;

/// Fetch the Minecraft versions from `repository` (normally [`versions::PAPER_REPO`]),
/// newest first.
pub async fn fetch_mc_versions(repository: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/{PAPER_METADATA_PATH}", repository.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("cannot reach {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| format!("{url}: {e}"))?;
    if bytes.len() > MAX_SIZE {
        return Err(format!("{url}: the answer is too large"));
    }
    let text =
        String::from_utf8(bytes.to_vec()).map_err(|_| format!("{url}: unexpected answer"))?;
    let found = mc_versions_from_paper_metadata(&text);
    if found.is_empty() {
        return Err(format!("{url}: no Minecraft versions found"));
    }
    Ok(found)
}

/// The default repository "Refresh versions" reads.
pub fn default_repository() -> &'static str {
    versions::PAPER_REPO
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A one-route HTTP server on localhost answering with `body` (or 404).
    async fn serve(path: &'static str, body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let ok = req.starts_with(&format!("GET {path} "));
                let (status, body) = if ok {
                    ("200 OK", body)
                } else {
                    ("404 Not Found", "")
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        url
    }

    #[tokio::test]
    async fn fetches_versions_from_a_repository() {
        let body = include_str!("../tests/fixtures/paper-api-maven-metadata.xml");
        let url = serve("/repo/io/papermc/paper/paper-api/maven-metadata.xml", body).await;
        let versions = fetch_mc_versions(&format!("{url}/repo/")).await.unwrap();
        assert_eq!(versions[0], "1.21.11");
        let err = fetch_mc_versions(&format!("{url}/missing"))
            .await
            .unwrap_err();
        assert!(err.contains("404"), "{err}");
    }
}
