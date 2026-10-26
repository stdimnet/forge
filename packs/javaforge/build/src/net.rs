//! Network access of JavaForge's build support, only from Rust: Maven Central search and
//! `maven-metadata.xml` for completion and the outdated-version inspection, cached in the
//! user cache dir (`<cache>/forge/maven-central`, never in the project) with a time to
//! live and the stale copy when offline; and services.gradle.org for Update Wrapper (the
//! newest release and the published SHA-256 sums).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use crate::central::{self, Artifact, Versions};
use crate::dsl::{CentralData, Request};
use crate::wrapper::{self, GradleRelease};

const MAX_SIZE: usize = 8 << 20;

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn client(direct: bool) -> Result<reqwest::Client, String> {
    let mut b = reqwest::Client::builder()
        .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30));
    if direct {
        b = b.no_proxy();
    }
    b.build().map_err(|e| e.to_string())
}

async fn get(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let r = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("cannot reach {url}: {e}"))?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("{url}: not found"));
    }
    if !r.status().is_success() {
        return Err(format!("{url}: HTTP {}", r.status()));
    }
    let bytes = r.bytes().await.map_err(|e| format!("{url}: {e}"))?;
    if bytes.len() > MAX_SIZE {
        return Err(format!("{url}: the answer is too large"));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| format!("{url}: unexpected answer"))
}

/// Maven Central, cached.
#[derive(Clone)]
pub struct MavenCentral {
    client: reqwest::Client,
    search_url: String,
    repository_url: String,
    cache: PathBuf,
    ttl: Duration,
}

impl MavenCentral {
    /// `search_url` / `repository_url`: the settings (`java.mavenCentral.*`).
    pub fn new(search_url: &str, repository_url: &str, cache: PathBuf) -> Result<Self, String> {
        Ok(MavenCentral {
            client: client(false)?,
            search_url: search_url.into(),
            repository_url: repository_url.into(),
            cache,
            ttl: Duration::from_secs(6 * 60 * 60),
        })
    }

    /// A client for a server on localhost (tests), without a proxy.
    pub fn direct(base: &str, cache: PathBuf, ttl: Duration) -> Result<Self, String> {
        Ok(MavenCentral {
            client: client(true)?,
            search_url: base.into(),
            repository_url: format!("{base}/maven2"),
            cache,
            ttl,
        })
    }

    /// `<cache dir>/forge/maven-central`.
    pub fn default_cache_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("forge").join("maven-central"))
    }

    fn fresh(&self, path: &Path) -> bool {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < self.ttl)
    }

    fn store(path: &Path, text: &str) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    /// The cached answer while fresh, else the server's (stored), else the stale cache.
    async fn cached<T>(
        &self,
        path: PathBuf,
        url: &str,
        parse: impl Fn(&str) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.fresh(&path) {
            if let Ok(v) = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| parse(&t))
            {
                return Ok(v);
            }
        }
        match get(&self.client, url).await {
            Ok(text) => {
                let v = parse(&text)?;
                Self::store(&path, &text);
                Ok(v)
            }
            Err(e) => std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| parse(&t).ok())
                .ok_or(e),
        }
    }

    /// Artifacts for what was typed (`guav`, `org.junit.jupiter:`).
    pub async fn search(&self, prefix: &str) -> Result<Vec<Artifact>, String> {
        let Some(query) = central::search_query(prefix) else {
            return Ok(Vec::new());
        };
        let rows = if query.starts_with("g:") { 100 } else { 30 };
        let url = central::search_url(&self.search_url, &query, rows);
        let path = self
            .cache
            .join("search")
            .join(format!("{}.json", &sha256_hex(query.as_bytes())[..32]));
        self.cached(path, &url, central::parse_search).await
    }

    /// The versions of `group:artifact`.
    pub async fn versions(&self, group: &str, artifact: &str) -> Result<Versions, String> {
        let url = central::metadata_url(&self.repository_url, group, artifact)
            .ok_or_else(|| format!("{group}:{artifact} is not a Maven coordinate"))?;
        let path = self
            .cache
            .join("metadata")
            .join(group)
            .join(format!("{artifact}.xml"));
        self.cached(path, &url, central::parse_metadata).await
    }

    /// The answer to a completion request (empty on errors: the DSL model still answers).
    pub async fn answer(&self, request: &Request) -> CentralData {
        match request {
            Request::Coordinates { prefix } => CentralData {
                artifacts: self.search(prefix).await.unwrap_or_default(),
                versions: None,
            },
            Request::Versions {
                group, artifact, ..
            } => CentralData {
                artifacts: Vec::new(),
                versions: self.versions(group, artifact).await.ok(),
            },
        }
    }

    /// The versions of many `group:artifact`s (for the inspection; unknown ones left out).
    pub async fn versions_of(&self, coordinates: &[String]) -> HashMap<String, Vec<String>> {
        let mut out = HashMap::new();
        for c in coordinates.iter().take(80) {
            if let Some((g, a)) = c.split_once(':') {
                if let Ok(v) = self.versions(g, a).await {
                    out.insert(c.clone(), v.versions);
                }
            }
        }
        out
    }
}

/// The newest Gradle release (`<services>/versions/current`).
pub async fn gradle_current(services: &str, direct: bool) -> Result<GradleRelease, String> {
    let c = client(direct)?;
    wrapper::parse_current(
        &get(
            &c,
            &format!("{}/versions/current", services.trim_end_matches('/')),
        )
        .await?,
    )
}

/// The SHA-256 sum Gradle publishes next to a distribution (`<url>.sha256`).
pub async fn gradle_sha256(distribution_url: &str, direct: bool) -> Result<String, String> {
    let c = client(direct)?;
    let text = get(&c, &format!("{distribution_url}.sha256")).await?;
    wrapper::parse_sha256(&text)
        .ok_or_else(|| format!("{distribution_url}.sha256 is not a SHA-256 sum"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::central::fixtures;
    use crate::testserver;

    #[tokio::test]
    async fn searches_and_caches() {
        let server = testserver::start().await;
        server.routes.set(
            "/solrsearch/select?q=guav&rows=30&wt=json",
            fixtures::SEARCH,
        );
        let cache = tempfile::tempdir().unwrap();
        let c = MavenCentral::direct(&server.url, cache.path().into(), Duration::from_secs(60))
            .unwrap();
        let hits = c.search("guav").await.unwrap();
        assert_eq!(hits[0].artifact, "guava");
        c.search("guav").await.unwrap();
        assert_eq!(
            server
                .routes
                .hits("/solrsearch/select?q=guav&rows=30&wt=json"),
            1
        );
        assert!(c.search("g").await.unwrap().is_empty());
        // Expired and offline: the stale copy; never cached: an error.
        let offline =
            MavenCentral::direct("http://127.0.0.1:9", cache.path().into(), Duration::ZERO)
                .unwrap();
        assert_eq!(offline.search("guav").await.unwrap().len(), 3);
        assert!(offline.search("jackson").await.is_err());
    }

    #[tokio::test]
    async fn versions_and_answers() {
        let server = testserver::start().await;
        server.routes.set(
            "/maven2/com/google/guava/guava/maven-metadata.xml",
            fixtures::METADATA,
        );
        let cache = tempfile::tempdir().unwrap();
        let c = MavenCentral::direct(&server.url, cache.path().into(), Duration::from_secs(60))
            .unwrap();
        let v = c.versions("com.google.guava", "guava").await.unwrap();
        assert_eq!(v.versions.len(), 8);
        assert!(cache
            .path()
            .join("metadata/com.google.guava/guava.xml")
            .is_file());
        assert!(c.versions("../x", "y").await.is_err());
        let data = c
            .answer(&Request::Versions {
                group: "com.google.guava".into(),
                artifact: "guava".into(),
                prefix: String::new(),
            })
            .await;
        assert!(data.versions.is_some());
        let all = c
            .versions_of(&["com.google.guava:guava".into(), "acme:missing".into()])
            .await;
        assert_eq!(all.len(), 1);
        let missing = c
            .answer(&Request::Coordinates {
                prefix: "zz".into(),
            })
            .await;
        assert!(missing.artifacts.is_empty());
    }

    #[tokio::test]
    async fn gradle_services() {
        let server = testserver::start().await;
        let dist = format!("{}/distributions/gradle-8.11-bin.zip", server.url);
        server.routes.set(
            "/versions/current",
            format!(r#"{{"version":"8.11","downloadUrl":"{dist}","checksumUrl":"{dist}.sha256"}}"#),
        );
        let sum = "b".repeat(64);
        server
            .routes
            .set("/distributions/gradle-8.11-bin.zip.sha256", sum.clone());
        let r = gradle_current(&server.url, true).await.unwrap();
        assert_eq!(r.version, "8.11");
        assert_eq!(gradle_sha256(&r.download_url, true).await.unwrap(), sum);
        server
            .routes
            .set("/distributions/gradle-8.11-bin.zip.sha256", "garbage");
        assert!(gradle_sha256(&r.download_url, true).await.is_err());
    }
}
