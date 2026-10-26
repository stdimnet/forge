//! Module lookups for `go.mod` completion and "Add Dependency": versions from the Go module
//! proxy (`<proxy>/<escaped path>/@v/list`, `@latest`), cached in the user cache dir
//! (`<cache>/forge/go-proxy`, never in the project) for 12 hours, falling back to the cached
//! copy when the proxy cannot be reached; and the module paths already in the module cache
//! (`$GOMODCACHE/cache/download`).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use forge_langservers::managed::Http;
use goforge_lang_go::version::{escape_path, parse_version_list, unescape_path};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PROXY: &str = "https://proxy.golang.org";

#[derive(Clone)]
pub struct ModuleProxy {
    http: Http,
    base: String,
    cache: Option<PathBuf>,
    ttl: Duration,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Cached {
    versions: Vec<String>,
}

/// The first proxy of a `GOPROXY` list (`https://proxy.golang.org,direct`); `None` when
/// lookups are off (`off`, `direct`).
pub fn proxy_from_goproxy(goproxy: &str) -> Option<String> {
    let first = goproxy
        .split([',', '|'])
        .map(str::trim)
        .find(|p| !p.is_empty())
        .unwrap_or(DEFAULT_PROXY);
    (first != "off" && first != "direct").then(|| first.trim_end_matches('/').to_string())
}

impl ModuleProxy {
    pub fn new(base: &str, cache: Option<PathBuf>) -> Result<Self, String> {
        Ok(ModuleProxy {
            http: Http::new()?,
            base: base.trim_end_matches('/').to_string(),
            cache,
            ttl: Duration::from_secs(12 * 60 * 60),
        })
    }

    /// A client for a local server (tests).
    pub fn direct(base: &str, cache: Option<PathBuf>, ttl: Duration) -> Result<Self, String> {
        Ok(ModuleProxy {
            http: Http::direct()?,
            base: base.trim_end_matches('/').to_string(),
            cache,
            ttl,
        })
    }

    /// `<cache dir>/forge/go-proxy`.
    pub fn default_cache_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("forge").join("go-proxy"))
    }

    fn cache_file(&self, module: &str) -> Option<PathBuf> {
        let key: String = escape_path(module)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '!' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Some(self.cache.as_ref()?.join(format!("{key}.json")))
    }

    fn read_cache(&self, module: &str, fresh_only: bool) -> Option<Vec<String>> {
        let file = self.cache_file(module)?;
        if fresh_only {
            let age = std::fs::metadata(&file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())?;
            if age > self.ttl {
                return None;
            }
        }
        let c: Cached = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
        Some(c.versions)
    }

    fn write_cache(&self, module: &str, versions: &[String]) {
        if let Some(file) = self.cache_file(module) {
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let c = Cached {
                versions: versions.to_vec(),
            };
            let _ = std::fs::write(file, serde_json::to_string(&c).unwrap_or_default());
        }
    }

    /// The released versions of a module, newest first (pseudo-versions are not listed by
    /// the proxy; `@latest` fills in when there are no tags).
    pub async fn versions(&self, module: &str) -> Result<Vec<String>, String> {
        let module = module.trim();
        if module.is_empty() || module.contains(char::is_whitespace) || module.contains("..") {
            return Err(format!("not a module path: '{module}'"));
        }
        if let Some(v) = self.read_cache(module, true) {
            return Ok(v);
        }
        let escaped = escape_path(module);
        let result = async {
            let list = self
                .http
                .text(&format!("{}/{escaped}/@v/list", self.base))
                .await?;
            let mut versions = parse_version_list(&list);
            if versions.is_empty() {
                #[derive(Deserialize)]
                #[serde(rename_all = "PascalCase")]
                struct Info {
                    version: String,
                }
                if let Ok(i) = self
                    .http
                    .json::<Info>(&format!("{}/{escaped}/@latest", self.base))
                    .await
                {
                    versions.push(i.version);
                }
            }
            Ok::<_, String>(versions)
        }
        .await;
        match result {
            Ok(v) => {
                self.write_cache(module, &v);
                Ok(v)
            }
            Err(e) => self.read_cache(module, false).ok_or(e),
        }
    }
}

/// Module paths in the module cache (`<GOMODCACHE>/cache/download/<escaped path>/@v`),
/// sorted; at most `limit`.
pub fn cached_modules(gomodcache: &Path, limit: usize) -> Vec<String> {
    let root = gomodcache.join("cache").join("download");
    let mut out = Vec::new();
    let mut stack = vec![(root.clone(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if out.len() >= limit || depth > 8 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        children.sort();
        for c in children.into_iter().rev() {
            if c.file_name().is_some_and(|n| n == "@v") {
                if let Ok(rel) = dir.strip_prefix(&root) {
                    let escaped = rel.to_string_lossy().replace('\\', "/");
                    if let Some(path) = unescape_path(&escaped) {
                        out.push(path);
                    }
                }
            } else if c.file_name().is_some_and(|n| n != "sumdb") {
                stack.push((c, depth + 1));
            }
        }
    }
    out.sort();
    out.dedup();
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testserver;

    #[tokio::test]
    async fn versions_are_cached_and_survive_an_offline_proxy() {
        let server = testserver::start().await;
        server.routes.set(
            "/github.com/!azure/sdk/@v/list",
            "v1.0.0\nv1.2.0\nv1.10.0\n",
        );
        server.routes.set(
            "/example.com/untagged/@latest",
            r#"{"Version":"v0.0.0-20240101120000-abcdef123456","Time":"2024-01-01T12:00:00Z"}"#,
        );
        server.routes.set("/example.com/untagged/@v/list", "");
        let cache = tempfile::tempdir().unwrap();
        let p = ModuleProxy::direct(
            &server.url,
            Some(cache.path().to_path_buf()),
            Duration::from_secs(3600),
        )
        .unwrap();
        assert_eq!(
            p.versions("github.com/Azure/sdk").await.unwrap(),
            ["v1.10.0", "v1.2.0", "v1.0.0"]
        );
        assert_eq!(p.versions("github.com/Azure/sdk").await.unwrap().len(), 3);
        assert_eq!(
            server.routes.hits("/github.com/!azure/sdk/@v/list"),
            1,
            "cached"
        );
        assert_eq!(
            p.versions("example.com/untagged").await.unwrap(),
            ["v0.0.0-20240101120000-abcdef123456"]
        );
        // Expired cache and an unreachable proxy: the cached copy answers.
        let stale = ModuleProxy::direct(
            "http://127.0.0.1:9",
            Some(cache.path().to_path_buf()),
            Duration::ZERO,
        )
        .unwrap();
        assert_eq!(
            stale.versions("github.com/Azure/sdk").await.unwrap().len(),
            3
        );
        assert!(stale.versions("example.com/never").await.is_err());
        assert!(p.versions("a b").await.is_err());
    }

    #[test]
    fn lists_the_module_cache_and_reads_goproxy() {
        let t = tempfile::tempdir().unwrap();
        for d in [
            "cache/download/github.com/!burnt!sushi/toml/@v",
            "cache/download/golang.org/x/text/@v",
            "cache/download/sumdb/sum.golang.org/lookup",
        ] {
            std::fs::create_dir_all(t.path().join(d)).unwrap();
        }
        assert_eq!(
            cached_modules(t.path(), 10),
            ["github.com/BurntSushi/toml", "golang.org/x/text"]
        );
        assert_eq!(cached_modules(t.path(), 1).len(), 1);
        assert_eq!(
            proxy_from_goproxy("https://goproxy.io/,direct").as_deref(),
            Some("https://goproxy.io")
        );
        assert_eq!(proxy_from_goproxy("").as_deref(), Some(DEFAULT_PROXY));
        assert_eq!(proxy_from_goproxy("off"), None);
    }
}
