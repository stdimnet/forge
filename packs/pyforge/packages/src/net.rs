//! PyPI requests, only from Rust: package metadata and search results cached in the user
//! cache dir (`<cache>/forge/pypi`, never in the project) with a time to live, falling back
//! to the cached copy when PyPI cannot be reached.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pyforge_lang_python::pep508::{is_name, normalize};

use crate::pypi::{self, PackageInfo, SearchHit};

/// Larger responses are refused (the JSON of packages with thousands of releases is a few MB).
const MAX_SIZE: usize = 32 << 20;

#[derive(Clone)]
pub struct Pypi {
    client: reqwest::Client,
    index: String,
    cache: PathBuf,
    package_ttl: Duration,
    search_ttl: Duration,
}

impl Pypi {
    /// `index` (PyPI or a mirror) through the system proxy settings, cached in `cache`.
    pub fn new(index: &str, cache: PathBuf) -> Result<Pypi, String> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Pypi {
            client,
            index: index.trim_end_matches('/').to_string(),
            cache,
            package_ttl: Duration::from_secs(12 * 60 * 60),
            search_ttl: Duration::from_secs(60 * 60),
        })
    }

    /// A client for a local server without a proxy (tests).
    pub fn direct(index: &str, cache: PathBuf, ttl: Duration) -> Result<Pypi, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Pypi {
            client,
            index: index.trim_end_matches('/').to_string(),
            cache,
            package_ttl: ttl,
            search_ttl: ttl,
        })
    }

    /// `<cache dir>/forge/pypi`.
    pub fn default_cache_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("forge").join("pypi"))
    }

    async fn get(&self, url: &str) -> Result<String, String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("cannot reach PyPI: {e}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err("not found on PyPI".into());
        }
        if !response.status().is_success() {
            return Err(format!("PyPI answered HTTP {}", response.status()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("cannot read the PyPI response: {e}"))?;
        if bytes.len() > MAX_SIZE {
            return Err("the PyPI response is too large".into());
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| "unexpected PyPI response".into())
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

    fn package_path(&self, name: &str) -> PathBuf {
        self.cache
            .join("json")
            .join(format!("{}.json", normalize(name)))
    }

    /// The cached summary of a package, however old.
    pub fn cached(&self, name: &str) -> Option<PackageInfo> {
        if !is_name(name) {
            return None;
        }
        serde_json::from_str(&std::fs::read_to_string(self.package_path(name)).ok()?).ok()
    }

    /// Releases and summary of a package: the cache while fresh, else PyPI (the stale cache
    /// when PyPI cannot be reached).
    pub async fn package(&self, name: &str) -> Result<PackageInfo, String> {
        if !is_name(name) {
            return Err(format!("{name} is not a package name"));
        }
        let path = self.package_path(name);
        if Self::fresh(&path, self.package_ttl) {
            if let Some(info) = self.cached(name) {
                return Ok(info);
            }
        }
        match self.get(&pypi::json_url(&self.index, name)).await {
            Ok(text) => {
                let info = pypi::parse_json(&text)?;
                if let Ok(json) = serde_json::to_string(&info) {
                    Self::store(&path, &json);
                }
                Ok(info)
            }
            Err(e) => self.cached(name).ok_or(e),
        }
    }

    /// Packages matching `query`: an exact name first, then PyPI's search results.
    pub async fn search(&self, query: &str) -> Result<Vec<SearchHit>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let key: String = query
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .take(64)
            .collect();
        let path = self.cache.join("search").join(format!("{key}.json"));
        if Self::fresh(&path, self.search_ttl) {
            if let Some(hits) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<Vec<SearchHit>>(&t).ok())
            {
                return Ok(hits);
            }
        }
        let mut hits: Vec<SearchHit> = Vec::new();
        let exact = if is_name(query) {
            self.package(query).await.ok()
        } else {
            None
        };
        if let Some(p) = &exact {
            hits.push(SearchHit {
                name: p.name.clone(),
                version: p.version.clone(),
                summary: p.summary.clone(),
            });
        }
        match self.get(&pypi::search_url(&self.index, query)).await {
            Ok(html) => {
                for h in pypi::parse_search(&html) {
                    if !hits
                        .iter()
                        .any(|x| normalize(&x.name) == normalize(&h.name))
                    {
                        hits.push(h);
                    }
                }
            }
            Err(e) if hits.is_empty() => {
                return std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| serde_json::from_str::<Vec<SearchHit>>(&t).ok())
                    .ok_or(e);
            }
            Err(_) => {}
        }
        if let Ok(json) = serde_json::to_string(&hits) {
            Self::store(&path, &json);
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypi::tests::{REQUESTS_JSON, SEARCH_HTML};
    use crate::testserver;

    #[tokio::test]
    async fn caches_package_metadata() {
        let server = testserver::start().await;
        server.routes.set("/pypi/requests/json", REQUESTS_JSON);
        let tmp = tempfile::tempdir().unwrap();
        let pypi = Pypi::direct(
            &server.url,
            tmp.path().to_path_buf(),
            Duration::from_secs(3600),
        )
        .unwrap();
        let p = pypi.package("Requests").await.unwrap();
        assert_eq!(p.version, "2.32.3");
        let again = pypi.package("requests").await.unwrap();
        assert_eq!(again, p);
        assert_eq!(server.routes.hits("/pypi/requests/json"), 1);
        assert!(pypi.package("missing-pkg").await.is_err());
        assert!(pypi.package("not a name").await.is_err());

        // Expired cache and PyPI unreachable: the stale copy is used.
        let offline = Pypi::direct(
            "http://127.0.0.1:9",
            tmp.path().to_path_buf(),
            Duration::ZERO,
        )
        .unwrap();
        assert_eq!(offline.package("requests").await.unwrap().version, "2.32.3");
    }

    #[tokio::test]
    async fn searches_with_the_exact_name_first() {
        let server = testserver::start().await;
        server.routes.set("/pypi/requests/json", REQUESTS_JSON);
        server.routes.set("/search/?q=requests", SEARCH_HTML);
        let tmp = tempfile::tempdir().unwrap();
        let pypi = Pypi::direct(
            &server.url,
            tmp.path().to_path_buf(),
            Duration::from_secs(3600),
        )
        .unwrap();
        let hits = pypi.search("requests").await.unwrap();
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["requests", "requests-oauthlib"]);
        pypi.search("requests").await.unwrap();
        assert_eq!(server.routes.hits("/search/?q=requests"), 1);
        assert!(pypi.search("  ").await.unwrap().is_empty());
    }
}
