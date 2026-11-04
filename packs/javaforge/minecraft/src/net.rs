//! "Refresh versions" (feature `net`): read the Minecraft versions Paper publishes its API
//! for, and the newest build-numbered Paper and Purpur APIs (26.1+), from the
//! `maven-metadata.xml` files of their repositories. Only runs when the user asks for it.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::versions::{
    self, mc_versions_from_paper_metadata, newest_builds, Builds, PAPER_METADATA_PATH,
    PURPUR_METADATA_PATH,
};

const MAX_SIZE: usize = 4 << 20;

/// What a refresh found (cached by the app).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Refreshed {
    /// Newest first.
    pub mc_versions: Vec<String>,
    pub builds: Builds,
}

/// Refresh from Paper's repository (versions and Paper builds) and Purpur's (Purpur builds;
/// a failure there keeps the pinned Purpur builds).
pub async fn refresh(paper_repo: &str, purpur_repo: &str) -> Result<Refreshed, String> {
    let paper = fetch_text(paper_repo, PAPER_METADATA_PATH).await?;
    let mc_versions = mc_versions_from_paper_metadata(&paper);
    if mc_versions.is_empty() {
        return Err(format!("{paper_repo}: no Minecraft versions found"));
    }
    let purpur = fetch_text(purpur_repo, PURPUR_METADATA_PATH)
        .await
        .map(|xml| newest_builds(&xml))
        .unwrap_or_default();
    Ok(Refreshed {
        mc_versions,
        builds: Builds {
            paper: newest_builds(&paper),
            purpur,
        },
    })
}

/// Fetch the Minecraft versions from `repository` (normally [`versions::PAPER_REPO`]),
/// newest first.
pub async fn fetch_mc_versions(repository: &str) -> Result<Vec<String>, String> {
    let text = fetch_text(repository, PAPER_METADATA_PATH).await?;
    let found = mc_versions_from_paper_metadata(&text);
    if found.is_empty() {
        return Err(format!("{repository}: no Minecraft versions found"));
    }
    Ok(found)
}

async fn fetch_text(repository: &str, path: &str) -> Result<String, String> {
    let url = format!("{}/{path}", repository.trim_end_matches('/'));
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
    String::from_utf8(bytes.to_vec()).map_err(|_| format!("{url}: unexpected answer"))
}

/// The default repositories "Refresh versions" reads (Paper, Purpur).
pub fn default_repositories() -> (&'static str, &'static str) {
    (versions::PAPER_REPO, versions::PURPUR_REPO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// An HTTP server on localhost answering `routes` (path, body), else 404.
    async fn serve(routes: &'static [(&'static str, &'static str)]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let hit = routes
                    .iter()
                    .find(|(path, _)| req.starts_with(&format!("GET {path} ")));
                let (status, body) = match hit {
                    Some((_, body)) => ("200 OK", *body),
                    None => ("404 Not Found", ""),
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

    const PAPER: &str = include_str!("../tests/fixtures/paper-api-maven-metadata.xml");
    const PURPUR: &str = include_str!("../tests/fixtures/purpur-api-maven-metadata.xml");

    #[tokio::test]
    async fn fetches_versions_from_a_repository() {
        let url = serve(&[("/repo/io/papermc/paper/paper-api/maven-metadata.xml", PAPER)]).await;
        let versions = fetch_mc_versions(&format!("{url}/repo/")).await.unwrap();
        assert_eq!(versions[0], "26.3");
        let err = fetch_mc_versions(&format!("{url}/missing"))
            .await
            .unwrap_err();
        assert!(err.contains("404"), "{err}");
    }

    #[tokio::test]
    async fn refreshes_versions_and_builds() {
        let url = serve(&[
            (
                "/paper/io/papermc/paper/paper-api/maven-metadata.xml",
                PAPER,
            ),
            (
                "/purpur/org/purpurmc/purpur/purpur-api/maven-metadata.xml",
                PURPUR,
            ),
        ])
        .await;
        let r = refresh(&format!("{url}/paper"), &format!("{url}/purpur"))
            .await
            .unwrap();
        assert_eq!(r.mc_versions[0], "26.3");
        assert_eq!(r.builds.paper["26.2"], "26.2.build.150-stable");
        assert_eq!(r.builds.purpur["26.3"], "26.3.build.2655-stable");
        // Purpur unreachable: Paper's answer still counts.
        let r = refresh(&format!("{url}/paper"), &format!("{url}/nowhere"))
            .await
            .unwrap();
        assert!(r.builds.purpur.is_empty());
    }
}
