//! The NuGet tool window's client of a NuGet v3 feed (nuget.org by default): search,
//! versions of a package and the updates of installed packages. Versions are cached in
//! the user cache dir (`<cache>/forge/nuget/<feed>/<id>.json`, never in the project) for
//! six hours, and the cached copy is used when the feed cannot be reached.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use dotnetforge_lang_dotnet::nuget::{
    parse_search, parse_service_index, parse_versions, search_url, versions_url, SearchResult,
    ServiceIndex,
};
use dotnetforge_lang_dotnet::version::upgrade;
use forge_langservers::managed::Http;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct NuGetClient {
    http: Http,
    index_url: String,
    cache: Option<PathBuf>,
    ttl: Duration,
    index: Arc<Mutex<Option<ServiceIndex>>>,
}

/// An installed package with a newer version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageUpdate {
    pub id: String,
    pub installed: String,
    pub latest: String,
}

#[derive(Serialize, Deserialize)]
struct Cached {
    versions: Vec<String>,
}

impl NuGetClient {
    /// A client of the feed whose service index is `index_url`, caching in
    /// `<cache dir>/forge/nuget`.
    pub fn new(index_url: &str) -> Result<Self, String> {
        Ok(Self::with(Http::new()?, index_url, default_cache_dir(index_url)))
    }

    /// A client for a local server (tests).
    pub fn direct(index_url: &str, cache: Option<PathBuf>) -> Result<Self, String> {
        Ok(Self::with(Http::direct()?, index_url, cache))
    }

    fn with(http: Http, index_url: &str, cache: Option<PathBuf>) -> Self {
        NuGetClient {
            http,
            index_url: index_url.trim().to_string(),
            cache,
            ttl: Duration::from_secs(6 * 60 * 60),
            index: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn service_index(&self) -> Result<ServiceIndex, String> {
        let mut guard = self.index.lock().await;
        if let Some(i) = guard.as_ref() {
            return Ok(i.clone());
        }
        let text = self.http.text(&self.index_url).await?;
        let index = parse_service_index(&text)?;
        *guard = Some(index.clone());
        Ok(index)
    }

    /// Search the feed (`take` packages from `skip`).
    pub async fn search(
        &self,
        query: &str,
        skip: u32,
        take: u32,
        prerelease: bool,
    ) -> Result<SearchResult, String> {
        let index = self.service_index().await?;
        let search = index
            .search
            .ok_or("the feed has no search service")?;
        let text = self
            .http
            .text(&search_url(&search, query, skip, take.clamp(1, 100), prerelease))
            .await?;
        parse_search(&text)
    }

    fn cache_file(&self, id: &str) -> Option<PathBuf> {
        self.cache
            .as_ref()
            .map(|c| c.join(format!("{}.json", id.to_ascii_lowercase())))
    }

    fn read_cache(&self, id: &str, fresh_only: bool) -> Option<Vec<String>> {
        let f = self.cache_file(id)?;
        if fresh_only {
            let age = std::fs::metadata(&f)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())?;
            if age > self.ttl {
                return None;
            }
        }
        let text = std::fs::read_to_string(f).ok()?;
        serde_json::from_str::<Cached>(&text).ok().map(|c| c.versions)
    }

    /// The versions of a package, oldest first.
    pub async fn versions(&self, id: &str) -> Result<Vec<String>, String> {
        if let Some(v) = self.read_cache(id, true) {
            return Ok(v);
        }
        let fetched = async {
            let index = self.service_index().await?;
            let base = index
                .package_base
                .ok_or("the feed has no package base address")?;
            let text = self.http.text(&versions_url(&base, id)).await?;
            parse_versions(&text)
        }
        .await;
        match fetched {
            Ok(v) => {
                if let Some(f) = self.cache_file(id) {
                    if let Some(parent) = f.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Ok(t) = serde_json::to_string(&Cached { versions: v.clone() }) {
                        let _ = std::fs::write(f, t);
                    }
                }
                Ok(v)
            }
            Err(e) => self.read_cache(id, false).ok_or(e),
        }
    }

    /// Updates of `(id, installed version)` pairs; packages the feed does not know are
    /// skipped.
    pub async fn updates(
        &self,
        installed: &[(String, String)],
        prerelease: bool,
    ) -> Vec<PackageUpdate> {
        let mut out = Vec::new();
        for (id, version) in installed {
            if version.trim().is_empty() {
                continue;
            }
            let Ok(versions) = self.versions(id).await else {
                continue;
            };
            if let Some(latest) = upgrade(version, &versions, prerelease) {
                out.push(PackageUpdate {
                    id: id.clone(),
                    installed: version.clone(),
                    latest: latest.to_string(),
                });
            }
        }
        out
    }
}

/// `<cache dir>/forge/nuget/<feed hash>`.
pub fn default_cache_dir(index_url: &str) -> Option<PathBuf> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    index_url.trim().hash(&mut h);
    dirs::cache_dir().map(|d| d.join("forge").join("nuget").join(format!("{:016x}", h.finish())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_langservers::managed::testserver;

    fn fixture(name: &str) -> String {
        let p = format!(
            "{}/../lang-dotnet/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
    }

    async fn feed() -> testserver::TestServer {
        let server = testserver::start().await;
        server.routes.set(
            "/v3/index.json",
            format!(
                r#"{{"version":"3.0.0","resources":[{{"@id":"{0}/query","@type":"SearchQueryService/3.5.0"}},{{"@id":"{0}/flat/","@type":"PackageBaseAddress/3.0.0"}}]}}"#,
                server.url
            ),
        );
        server.routes.set(
            "/query?q=serilog&skip=0&take=20&prerelease=false&semVerLevel=2.0.0",
            fixture("nuget-search-serilog.json"),
        );
        server.routes.set(
            "/flat/swashbuckle.aspnetcore/index.json",
            r#"{"versions":["6.4.0","6.5.0","6.6.2","7.0.0-preview.1"]}"#,
        );
        server.routes.set(
            "/flat/xunit/index.json",
            r#"{"versions":["2.6.6","2.7.0"]}"#,
        );
        server
    }

    #[tokio::test]
    async fn searches_and_finds_updates() {
        let server = feed().await;
        let cache = tempfile::tempdir().unwrap();
        let client = NuGetClient::direct(&format!("{}/v3/index.json", server.url), Some(cache.path().to_path_buf())).unwrap();
        let r = client.search("serilog", 0, 20, false).await.unwrap();
        assert_eq!(r.packages[0].id, "Serilog");
        assert_eq!(r.total_hits, 1784);
        let installed = vec![
            ("Swashbuckle.AspNetCore".to_string(), "6.5.0".to_string()),
            ("xunit".to_string(), "2.7.0".to_string()),
            ("Unknown.Package".to_string(), "1.0.0".to_string()),
            ("Implicit".to_string(), String::new()),
        ];
        let u = client.updates(&installed, false).await;
        assert_eq!(
            u,
            vec![PackageUpdate { id: "Swashbuckle.AspNetCore".into(), installed: "6.5.0".into(), latest: "6.6.2".into() }]
        );
        // The service index is read once; versions come from the cache afterwards.
        client.versions("Swashbuckle.AspNetCore").await.unwrap();
        assert_eq!(server.routes.hits("/v3/index.json"), 1);
        assert_eq!(server.routes.hits("/flat/swashbuckle.aspnetcore/index.json"), 1);
        assert!(cache.path().join("swashbuckle.aspnetcore.json").is_file());
    }

    #[tokio::test]
    async fn falls_back_to_the_cache_offline() {
        let cache = tempfile::tempdir().unwrap();
        std::fs::write(cache.path().join("xunit.json"), r#"{"versions":["2.7.0","2.9.2"]}"#).unwrap();
        // A feed that does not answer.
        let client = NuGetClient::direct("http://127.0.0.1:9/v3/index.json", Some(cache.path().to_path_buf())).unwrap();
        let mut stale = client.clone();
        stale.ttl = Duration::ZERO;
        assert_eq!(stale.versions("xunit").await.unwrap().len(), 2);
        assert!(client.search("x", 0, 10, false).await.is_err());
        assert!(stale.versions("other").await.is_err());
        assert!(default_cache_dir("https://api.nuget.org/v3/index.json").is_some() || dirs::cache_dir().is_none());
    }
}
