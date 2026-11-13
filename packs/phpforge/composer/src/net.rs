//! Network access of the Composer support, only from Rust: Packagist metadata and search
//! cached in the user cache dir (`<cache>/forge/packagist`, never in the project) with a
//! time to live, falling back to the cached copy when offline; and the composer.phar
//! download from getcomposer.org, checked against the published SHA-256 sum, into Forge's
//! data dir (only when the user asks for it).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use crate::manifest::is_package_name;
use crate::packagist::{self, PackageInfo, SearchHit};

/// Larger responses are refused (laravel/framework's metadata is about 1 MB).
const MAX_SIZE: usize = 32 << 20;

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Clone)]
pub struct Packagist {
    client: reqwest::Client,
    repo_url: String,
    search_url: String,
    cache: PathBuf,
    package_ttl: Duration,
    search_ttl: Duration,
}

impl Packagist {
    /// Packagist.org through the system proxy settings, cached in `cache`.
    pub fn new(cache: PathBuf) -> Result<Packagist, String> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Packagist {
            client,
            repo_url: packagist::REPO_URL.into(),
            search_url: packagist::SEARCH_URL.into(),
            cache,
            package_ttl: Duration::from_secs(6 * 60 * 60),
            search_ttl: Duration::from_secs(60 * 60),
        })
    }

    /// A client for another server without a proxy (tests talk to localhost).
    pub fn direct(base: &str, cache: PathBuf, ttl: Duration) -> Result<Packagist, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Packagist {
            client,
            repo_url: base.into(),
            search_url: base.into(),
            cache,
            package_ttl: ttl,
            search_ttl: ttl,
        })
    }

    /// `<cache dir>/forge/packagist`.
    pub fn default_cache_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("forge").join("packagist"))
    }

    async fn get(&self, url: &str) -> Result<String, String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("cannot reach Packagist: {e}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err("not found on Packagist".into());
        }
        if !response.status().is_success() {
            return Err(format!("Packagist answered HTTP {}", response.status()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("cannot read the Packagist response: {e}"))?;
        if bytes.len() > MAX_SIZE {
            return Err("the Packagist response is too large".into());
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| "unexpected Packagist response".into())
    }

    fn fresh(path: &Path, ttl: Duration) -> bool {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < ttl)
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

    /// The cached summary of a package, however old.
    pub fn cached_package(&self, name: &str) -> Option<PackageInfo> {
        if !is_package_name(name) {
            return None;
        }
        let path = self.package_path(name);
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    fn package_path(&self, name: &str) -> PathBuf {
        let (vendor, package) = name.split_once('/').expect("validated package name");
        self.cache
            .join("p2")
            .join(vendor)
            .join(format!("{package}.json"))
    }

    /// Releases and description of a package: the cache while fresh, else Packagist (the
    /// stale cache when Packagist cannot be reached).
    pub async fn package(&self, name: &str) -> Result<PackageInfo, String> {
        if !is_package_name(name) {
            return Err(format!("{name} is not a package name"));
        }
        let path = self.package_path(name);
        if Self::fresh(&path, self.package_ttl) {
            if let Some(info) = self.cached_package(name) {
                return Ok(info);
            }
        }
        let url = packagist::p2_url(&self.repo_url, name);
        match self.get(&url).await {
            Ok(text) => {
                let info = packagist::parse_p2(name, &text)?;
                if let Ok(json) = serde_json::to_string(&info) {
                    Self::store(&path, &json);
                }
                Ok(info)
            }
            Err(e) => self.cached_package(name).ok_or(e),
        }
    }

    /// Packages matching `query`, most downloaded first (Packagist's order).
    pub async fn search(&self, query: &str, per_page: usize) -> Result<Vec<SearchHit>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let url = packagist::search_url(&self.search_url, query, per_page);
        let path = self.cache.join("search").join(format!(
            "{}.json",
            &sha256_hex(format!("{query}\n{per_page}").as_bytes())[..32]
        ));
        if Self::fresh(&path, self.search_ttl) {
            if let Ok(hits) =
                packagist::parse_search(&std::fs::read_to_string(&path).unwrap_or_default())
            {
                return Ok(hits);
            }
        }
        match self.get(&url).await {
            Ok(text) => {
                let hits = packagist::parse_search(&text)?;
                Self::store(&path, &text);
                Ok(hits)
            }
            Err(e) => std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| packagist::parse_search(&t).ok())
                .ok_or(e),
        }
    }
}

/// Where the composer.phar download comes from.
pub const GETCOMPOSER_URL: &str = "https://getcomposer.org";

/// `<data dir>/forge/composer`.
pub fn default_phar_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("composer"))
}

/// Download the latest stable composer.phar from `base` (getcomposer.org), check it
/// against the SHA-256 sum published next to it and save it as `dir/composer.phar`.
pub async fn download_phar(base: &str, dir: &Path, direct: bool) -> Result<PathBuf, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20));
    if direct {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| e.to_string())?;
    let get = |url: String| {
        let client = client.clone();
        async move {
            let r = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("cannot reach {url}: {e}"))?;
            if !r.status().is_success() {
                return Err(format!("{url}: HTTP {}", r.status()));
            }
            r.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| format!("download of {url} failed: {e}"))
        }
    };
    let base = base.trim_end_matches('/');
    let sums = get(format!(
        "{base}/download/latest-stable/composer.phar.sha256sum"
    ))
    .await?;
    let expected = String::from_utf8_lossy(&sums)
        .split_whitespace()
        .next()
        .map(str::to_ascii_lowercase)
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or("getcomposer.org published no SHA-256 sum")?;
    let phar = get(format!("{base}/download/latest-stable/composer.phar")).await?;
    let actual = sha256_hex(&phar);
    if actual != expected {
        return Err(format!(
            "the SHA-256 sum of composer.phar does not match ({actual} instead of {expected}); nothing was saved"
        ));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let target = dir.join("composer.phar");
    let tmp = dir.join("composer.phar.download");
    std::fs::write(&tmp, &phar).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &target)
        .map_err(|e| format!("cannot write {}: {e}", target.display()))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packagist::fixtures;
    use crate::testserver;

    #[tokio::test]
    async fn caches_package_metadata() {
        let server = testserver::start().await;
        server
            .routes
            .set("/p2/laravel/framework.json", fixtures::LARAVEL_P2);
        let cache = tempfile::tempdir().unwrap();
        let p =
            Packagist::direct(&server.url, cache.path().into(), Duration::from_secs(60)).unwrap();
        let info = p.package("laravel/framework").await.unwrap();
        assert_eq!(info.versions.len(), 5);
        // Fresh cache: no second request.
        p.package("laravel/framework").await.unwrap();
        assert_eq!(server.routes.hits("/p2/laravel/framework.json"), 1);
        assert!(cache.path().join("p2/laravel/framework.json").is_file());

        // Expired and offline: the stale copy.
        let stale =
            Packagist::direct("http://127.0.0.1:9", cache.path().into(), Duration::ZERO).unwrap();
        assert_eq!(stale.package("laravel/framework").await.unwrap(), info);
        // Never cached and offline: an error.
        assert!(stale.package("acme/missing").await.is_err());
        assert!(p.package("../etc/passwd").await.is_err());
        assert!(p
            .package("acme/missing")
            .await
            .unwrap_err()
            .contains("not found"));
    }

    #[tokio::test]
    async fn searches_and_caches() {
        let server = testserver::start().await;
        server
            .routes
            .set("/search.json?q=laravel&per_page=15", fixtures::SEARCH);
        let cache = tempfile::tempdir().unwrap();
        let p =
            Packagist::direct(&server.url, cache.path().into(), Duration::from_secs(60)).unwrap();
        let hits = p.search("laravel", 15).await.unwrap();
        assert_eq!(hits[0].name, "laravel/framework");
        p.search("laravel", 15).await.unwrap();
        assert_eq!(server.routes.hits("/search.json?q=laravel&per_page=15"), 1);
        assert!(p.search("  ", 15).await.unwrap().is_empty());
        let offline =
            Packagist::direct("http://127.0.0.1:9", cache.path().into(), Duration::ZERO).unwrap();
        assert_eq!(offline.search("laravel", 15).await.unwrap().len(), 3);
        assert!(offline.search("symfony", 15).await.is_err());
    }

    #[tokio::test]
    async fn downloads_composer_phar_with_checksum() {
        let server = testserver::start().await;
        let phar = b"<?php // fake composer.phar".to_vec();
        server
            .routes
            .set("/download/latest-stable/composer.phar", phar.clone());
        server.routes.set(
            "/download/latest-stable/composer.phar.sha256sum",
            format!("{}  composer.phar\n", sha256_hex(&phar)),
        );
        let dir = tempfile::tempdir().unwrap();
        let path = download_phar(&server.url, dir.path(), true).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), phar);

        server.routes.set(
            "/download/latest-stable/composer.phar.sha256sum",
            format!("{}  composer.phar\n", "0".repeat(64)),
        );
        let other = tempfile::tempdir().unwrap();
        let err = download_phar(&server.url, other.path(), true)
            .await
            .unwrap_err();
        assert!(err.contains("does not match"), "{err}");
        assert!(!other.path().join("composer.phar").exists());
    }
}
