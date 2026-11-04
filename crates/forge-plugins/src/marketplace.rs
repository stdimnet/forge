//! The marketplace client: a versioned index JSON at a configurable URL
//! (`plugins.marketplace.url`), search, update checks, and verified downloads.
//!
//! Index format 1:
//!
//! ```json
//! {
//!   "formatVersion": 1,
//!   "name": "Example plugins",
//!   "plugins": [{
//!     "id": "example.word-count", "name": "Word Count", "publisher": "Example",
//!     "description": "…", "tags": ["editor"], "downloads": 120,
//!     "versions": [{
//!       "version": "1.1.0", "engines": { "forge": ">=0.1.0" },
//!       "url": "word-count-1.1.0.forgeplugin", "size": 2048,
//!       "sha256": "<64 hex digits>", "signature": "<base64 Ed25519, optional>",
//!       "permissions": { "editor": ["read"] }, "released": "2026-10-01", "changes": "…"
//!     }]
//!   }]
//! }
//! ```
//!
//! Artifact URLs may be relative to the index. Every download is checked against its
//! `sha256`; when the user configured trusted publisher keys
//! (`plugins.marketplace.trustedKeys`), an Ed25519 signature over the package bytes from one
//! of them is required as well.

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::manifest::{is_plugin_id, Engines, Permissions};
use crate::sha256_hex;

/// The index format this Forge reads.
pub const INDEX_FORMAT: u32 = 1;
/// The default index URL: a placeholder, no marketplace is live there.
pub const PLACEHOLDER_URL: &str = "https://plugins.forge.example/index.json";
/// Largest index Forge reads.
const MAX_INDEX: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub format_version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub plugins: Vec<IndexPlugin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexPlugin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub downloads: u64,
    pub versions: Vec<IndexVersion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexVersion {
    pub version: String,
    pub engines: Engines,
    /// Absolute, or relative to the index URL.
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    /// Base64 Ed25519 signature of the package bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// The permissions the package's manifest declares; checked again after download.
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<String>,
}

impl Index {
    /// Parse an index; entries that break the format are dropped (one bad entry must not
    /// hide the whole marketplace) and reported as warnings.
    pub fn parse(json: &str) -> Result<(Self, Vec<String>), String> {
        if json.len() > MAX_INDEX {
            return Err("the marketplace index is larger than 16 MB".into());
        }
        let probe: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("invalid marketplace index: {e}"))?;
        let format = probe
            .get("formatVersion")
            .and_then(|v| v.as_u64())
            .ok_or("invalid marketplace index: `formatVersion` is missing")?;
        if format > INDEX_FORMAT as u64 {
            return Err(format!(
                "the marketplace index uses format {format}; this Forge reads format {INDEX_FORMAT}. Update Forge to browse it."
            ));
        }
        let mut index: Index =
            serde_json::from_value(probe).map_err(|e| format!("invalid marketplace index: {e}"))?;
        let mut warnings = Vec::new();
        index.plugins.retain_mut(|p| {
            if !is_plugin_id(&p.id) {
                warnings.push(format!("skipped `{}`: invalid id", p.id));
                return false;
            }
            p.versions.retain(|v| {
                let ok = semver::Version::parse(&v.version).is_ok()
                    && semver::VersionReq::parse(&v.engines.forge).is_ok()
                    && v.sha256.len() == 64
                    && v.sha256.bytes().all(|b| b.is_ascii_hexdigit());
                if !ok {
                    warnings.push(format!(
                        "skipped {} {}: invalid version entry",
                        p.id, v.version
                    ));
                }
                ok
            });
            !p.versions.is_empty()
        });
        Ok((index, warnings))
    }
}

/// An installed plugin as the marketplace sees it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    pub id: String,
    pub version: String,
}

/// A plugin of the Marketplace tab.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    pub tags: Vec<String>,
    pub downloads: u64,
    /// The newest version that works with this Forge (or the newest at all when none does).
    pub latest: IndexVersion,
    /// Whether `latest` works with this Forge.
    pub compatible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    pub update_available: bool,
    /// The permission prompt of `latest`.
    pub permission_lines: Vec<crate::permissions::PermissionLine>,
}

fn newest<'a>(versions: impl Iterator<Item = &'a IndexVersion>) -> Option<&'a IndexVersion> {
    versions.max_by(|a, b| {
        let va = semver::Version::parse(&a.version).ok();
        let vb = semver::Version::parse(&b.version).ok();
        va.cmp(&vb)
    })
}

fn works_with(v: &IndexVersion, forge: &semver::Version) -> bool {
    semver::VersionReq::parse(&v.engines.forge).is_ok_and(|r| r.matches(forge))
}

/// The entries of the index for this Forge version and the installed plugins.
pub fn entries(index: &Index, forge_version: &str, installed: &[InstalledVersion]) -> Vec<Entry> {
    let forge = semver::Version::parse(forge_version).unwrap_or(semver::Version::new(0, 0, 0));
    index
        .plugins
        .iter()
        .filter_map(|p| {
            let compatible = newest(p.versions.iter().filter(|v| works_with(v, &forge)));
            let latest = compatible.or_else(|| newest(p.versions.iter()))?.clone();
            let installed_version = installed
                .iter()
                .find(|i| i.id == p.id)
                .map(|i| i.version.clone());
            let update_available = compatible.is_some()
                && installed_version.as_deref().is_some_and(|iv| {
                    match (
                        semver::Version::parse(iv),
                        semver::Version::parse(&latest.version),
                    ) {
                        (Ok(a), Ok(b)) => b > a,
                        _ => false,
                    }
                });
            Some(Entry {
                id: p.id.clone(),
                name: p.name.clone(),
                publisher: p.publisher.clone(),
                description: p.description.clone(),
                homepage: p.homepage.clone(),
                license: p.license.clone(),
                tags: p.tags.clone(),
                downloads: p.downloads,
                permission_lines: crate::permissions::summary(&latest.permissions),
                compatible: compatible.is_some(),
                latest,
                installed_version,
                update_available,
            })
        })
        .collect()
}

/// Entries matching `query` (name, id, publisher, tags, description), best first; an empty
/// query lists everything by downloads.
pub fn search(entries: &[Entry], query: &str) -> Vec<Entry> {
    let q = query.trim().to_lowercase();
    let mut scored: Vec<(u32, &Entry)> = entries
        .iter()
        .filter_map(|e| {
            if q.is_empty() {
                return Some((0, e));
            }
            let name = e.name.to_lowercase();
            let score = if name == q {
                100
            } else if name.starts_with(&q) {
                80
            } else if name.contains(&q) || e.id.contains(&q) {
                60
            } else if e.tags.iter().any(|t| t.to_lowercase() == q) {
                50
            } else if e.publisher.to_lowercase().contains(&q) {
                40
            } else if e.description.to_lowercase().contains(&q) {
                20
            } else {
                return None;
            };
            Some((score, e))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.downloads.cmp(&a.1.downloads))
            .then(a.1.name.cmp(&b.1.name))
    });
    scored.into_iter().map(|(_, e)| e.clone()).collect()
}

/// The URL of an artifact: absolute, or relative to the index.
pub fn artifact_url(index_url: &str, artifact: &str) -> Result<String, String> {
    let base = url::Url::parse(index_url).map_err(|e| format!("invalid index URL: {e}"))?;
    let url = base
        .join(artifact)
        .map_err(|e| format!("invalid artifact URL {artifact}: {e}"))?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err(format!("unsupported artifact URL {url}"));
    }
    Ok(url.into())
}

/// Check downloaded package bytes against the index entry and the trusted keys.
pub fn verify(bytes: &[u8], version: &IndexVersion, trusted_keys: &[String]) -> Result<(), String> {
    if version.size > 0 && bytes.len() as u64 != version.size {
        return Err(format!(
            "the download has {} bytes, the marketplace announced {}",
            bytes.len(),
            version.size
        ));
    }
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(&version.sha256) {
        return Err(format!(
            "checksum mismatch: expected SHA-256 {}, got {actual}",
            version.sha256
        ));
    }
    if trusted_keys.is_empty() {
        return Ok(());
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    let signature = version
        .signature
        .as_deref()
        .ok_or("the package is not signed, but trusted publisher keys are configured")?;
    let signature: [u8; 64] = b64
        .decode(signature.trim())
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or("the package signature is malformed")?;
    let signature = ed25519_dalek::Signature::from_bytes(&signature);
    for key in trusted_keys {
        let Some(key) = b64
            .decode(key.trim())
            .ok()
            .and_then(|k| <[u8; 32]>::try_from(k).ok())
            .and_then(|k| ed25519_dalek::VerifyingKey::from_bytes(&k).ok())
        else {
            continue;
        };
        if key.verify_strict(bytes, &signature).is_ok() {
            return Ok(());
        }
    }
    Err("the package signature does not match any trusted publisher key".into())
}

/// HTTP access to a marketplace (system proxy settings apply).
#[cfg(feature = "net")]
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
}

#[cfg(feature = "net")]
impl Client {
    pub fn new() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { http })
    }

    /// A client without proxy (tests talk to localhost).
    pub fn direct() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { http })
    }

    async fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| format!("cannot reach {url}: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("{url}: HTTP {}", response.status()));
        }
        let mut data = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("download of {url} failed: {e}"))?
        {
            data.extend_from_slice(&chunk);
            if data.len() as u64 > limit {
                return Err(format!("{url} is larger than expected"));
            }
        }
        Ok(data)
    }

    pub async fn index(&self, url: &str) -> Result<(Index, Vec<String>), String> {
        let bytes = self.get(url, MAX_INDEX as u64).await?;
        Index::parse(&String::from_utf8_lossy(&bytes))
    }

    /// Download and verify one version of a plugin.
    pub async fn download(
        &self,
        index_url: &str,
        version: &IndexVersion,
        trusted_keys: &[String],
    ) -> Result<Vec<u8>, String> {
        let url = artifact_url(index_url, &version.url)?;
        let bytes = self.get(&url, crate::package::MAX_SIZE).await?;
        verify(&bytes, version, trusted_keys)?;
        Ok(bytes)
    }
}

/// Find a version of a plugin in the index (`None`: the newest compatible one).
pub fn find_version<'a>(
    index: &'a Index,
    id: &str,
    version: Option<&str>,
    forge_version: &str,
) -> Result<&'a IndexVersion, String> {
    let plugin = index
        .plugins
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("`{id}` is not in the marketplace"))?;
    let forge = semver::Version::parse(forge_version).unwrap_or(semver::Version::new(0, 0, 0));
    match version {
        Some(v) => plugin
            .versions
            .iter()
            .find(|x| x.version == v)
            .ok_or_else(|| format!("version {v} of `{id}` is not in the marketplace")),
        None => newest(plugin.versions.iter().filter(|v| works_with(v, &forge)))
            .ok_or_else(|| format!("no version of `{id}` works with Forge {forge_version}")),
    }
}

#[cfg(all(test, feature = "net"))]
mod tests {
    use super::*;
    use crate::package::tests::word_count_package;
    use ed25519_dalek::Signer;
    use forge_langservers::managed::testserver;

    fn version(v: &str, bytes: &[u8], engine: &str) -> serde_json::Value {
        serde_json::json!({
            "version": v, "engines": { "forge": engine },
            "url": format!("files/word-count-{v}.forgeplugin"),
            "sha256": sha256_hex(bytes), "size": bytes.len(),
            "permissions": { "editor": ["read"] }
        })
    }

    fn index_json(v1: &[u8], v2: &[u8]) -> String {
        serde_json::json!({
            "formatVersion": 1,
            "name": "Test",
            "plugins": [
                {
                    "id": "example.word-count", "name": "Word Count", "publisher": "Example",
                    "description": "Counts words", "tags": ["editor"], "downloads": 10,
                    "versions": [version("1.0.0", v1, ">=0.1.0"), version("1.1.0", v2, ">=0.1.0"),
                                 version("2.0.0", v2, ">=5.0.0")]
                },
                {
                    "id": "acme.todo-sync", "name": "TODO Sync", "publisher": "Acme",
                    "description": "Syncs TODO comments with a word tracker", "downloads": 99,
                    "versions": [version("0.3.0", v1, ">=0.1.0")]
                },
                { "id": "Bad Id", "name": "x", "versions": [version("1.0.0", v1, "*")] },
                { "id": "acme.broken", "name": "Broken",
                  "versions": [{ "version": "x", "engines": { "forge": "*" }, "url": "a", "sha256": "00" }] }
            ]
        })
        .to_string()
    }

    #[test]
    fn parses_and_lists_entries() {
        let (v1, v2) = (word_count_package("1.0.0"), word_count_package("1.1.0"));
        let (index, warnings) = Index::parse(&index_json(&v1, &v2)).unwrap();
        assert_eq!(index.plugins.len(), 2);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        let installed = [InstalledVersion {
            id: "example.word-count".into(),
            version: "1.0.0".into(),
        }];
        let list = entries(&index, "0.1.0", &installed);
        let wc = &list[0];
        assert_eq!(wc.latest.version, "1.1.0", "2.0.0 needs a newer Forge");
        assert!(wc.compatible && wc.update_available);
        assert_eq!(wc.installed_version.as_deref(), Some("1.0.0"));
        assert_eq!(wc.permission_lines[0].kind, "editor.read");
        assert!(!list[1].update_available);
        // With a newer Forge 2.0.0 is the latest.
        assert_eq!(entries(&index, "5.0.0", &[])[0].latest.version, "2.0.0");
        assert_eq!(
            find_version(&index, "example.word-count", None, "0.1.0")
                .unwrap()
                .version,
            "1.1.0"
        );
        assert!(find_version(&index, "nope.nope", None, "0.1.0").is_err());
    }

    #[test]
    fn search_ranks_names_first() {
        let (v1, v2) = (word_count_package("1.0.0"), word_count_package("1.1.0"));
        let (index, _) = Index::parse(&index_json(&v1, &v2)).unwrap();
        let list = entries(&index, "0.1.0", &[]);
        let ids = |q: &str| -> Vec<String> { search(&list, q).into_iter().map(|e| e.id).collect() };
        // "word" is in Word Count's name and only in TODO Sync's description.
        assert_eq!(ids("word"), ["example.word-count", "acme.todo-sync"]);
        assert_eq!(ids("acme"), ["acme.todo-sync"]);
        assert_eq!(ids(""), ["acme.todo-sync", "example.word-count"]);
        assert!(ids("zzz").is_empty());
    }

    #[test]
    fn refuses_newer_formats() {
        let e = Index::parse(r#"{ "formatVersion": 2, "plugins": [] }"#).unwrap_err();
        assert!(e.contains("format 2"));
        assert!(Index::parse("{}").is_err());
    }

    #[test]
    fn urls_and_checksums() {
        assert_eq!(
            artifact_url("https://m.example/v1/index.json", "files/a.forgeplugin").unwrap(),
            "https://m.example/v1/files/a.forgeplugin"
        );
        assert_eq!(
            artifact_url("https://m.example/index.json", "https://cdn.example/a").unwrap(),
            "https://cdn.example/a"
        );
        assert!(artifact_url("https://m.example/index.json", "file:///etc/passwd").is_err());
        let bytes = word_count_package("1.0.0");
        let v: IndexVersion = serde_json::from_value(version("1.0.0", &bytes, ">=0.1.0")).unwrap();
        assert!(verify(&bytes, &v, &[]).is_ok());
        let mut tampered = bytes.clone();
        tampered[10] ^= 1;
        assert!(verify(&tampered, &v, &[]).unwrap_err().contains("checksum"));
    }

    #[test]
    fn signatures_from_trusted_keys() {
        let b64 = base64::engine::general_purpose::STANDARD;
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let other = ed25519_dalek::SigningKey::from_bytes(&[8u8; 32]);
        let trusted = vec![b64.encode(key.verifying_key().as_bytes())];
        let bytes = word_count_package("1.0.0");
        let mut v: IndexVersion =
            serde_json::from_value(version("1.0.0", &bytes, ">=0.1.0")).unwrap();
        assert!(verify(&bytes, &v, &trusted)
            .unwrap_err()
            .contains("not signed"));
        v.signature = Some(b64.encode(other.sign(&bytes).to_bytes()));
        assert!(verify(&bytes, &v, &trusted)
            .unwrap_err()
            .contains("does not match"));
        v.signature = Some(b64.encode(key.sign(&bytes).to_bytes()));
        assert!(verify(&bytes, &v, &trusted).is_ok());
    }

    #[tokio::test]
    async fn downloads_from_a_local_marketplace() {
        let (v1, v2) = (word_count_package("1.0.0"), word_count_package("1.1.0"));
        let server = testserver::start().await;
        server.routes.set("/index.json", index_json(&v1, &v2));
        server
            .routes
            .set("/files/word-count-1.1.0.forgeplugin", v2.clone());
        server
            .routes
            .set("/files/word-count-1.0.0.forgeplugin", v2.clone());
        let client = Client::direct().unwrap();
        let url = format!("{}/index.json", server.url);
        let (index, _) = client.index(&url).await.unwrap();
        let latest = find_version(&index, "example.word-count", None, "0.1.0").unwrap();
        let bytes = client.download(&url, latest, &[]).await.unwrap();
        assert_eq!(bytes, v2);
        let pkg = crate::package::Package::read(&bytes, "0.1.0").unwrap();
        assert_eq!(pkg.manifest.version, "1.1.0");
        // The server hands out the wrong file for 1.0.0: the checksum catches it.
        let old = find_version(&index, "example.word-count", Some("1.0.0"), "0.1.0").unwrap();
        assert!(client.download(&url, old, &[]).await.is_err());
        assert!(client
            .index(&format!("{}/missing.json", server.url))
            .await
            .is_err());
    }
}
