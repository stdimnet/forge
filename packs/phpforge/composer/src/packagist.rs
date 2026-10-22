//! Packagist metadata: the Composer 2 repository format
//! (`https://repo.packagist.org/p2/<vendor>/<package>.json`, "minified" version lists) and
//! the search API (`https://packagist.org/search.json?q=…`), reduced to what Forge shows.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::constraint::recommended;
use crate::version::Version;

pub const REPO_URL: &str = "https://repo.packagist.org";
pub const SEARCH_URL: &str = "https://packagist.org";

/// A package with its released versions, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// The replacement of an abandoned package (empty: abandoned without one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abandoned: Option<String>,
    pub versions: Vec<String>,
}

impl PackageInfo {
    /// The newest stable version (else the newest pre-release).
    pub fn latest(&self) -> Option<Version> {
        let all = self.versions.iter().map(String::as_str);
        crate::version::latest(all.clone(), true).or_else(|| crate::version::latest(all, false))
    }

    /// What the version dropdown offers: the recommended constraint of the latest
    /// version, then the versions.
    pub fn suggested_constraint(&self) -> Option<String> {
        self.latest().map(|v| recommended(&v))
    }

    pub fn page_url(&self) -> String {
        format!("https://packagist.org/packages/{}", self.name)
    }
}

/// Expand Composer 2's minified version list: each entry only has the fields that differ
/// from the one before it, and `"__unset"` removes a field.
pub fn expand_minified(entries: &[Value]) -> Vec<Map<String, Value>> {
    let mut out = Vec::with_capacity(entries.len());
    let mut current = Map::new();
    for e in entries {
        let Some(obj) = e.as_object() else { continue };
        for (k, v) in obj {
            if v.as_str() == Some("__unset") {
                current.remove(k);
            } else {
                current.insert(k.clone(), v.clone());
            }
        }
        out.push(current.clone());
    }
    out
}

/// Parse `p2/<name>.json`.
pub fn parse_p2(name: &str, json: &str) -> Result<PackageInfo, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("unexpected response: {e}"))?;
    let entries = v
        .get("packages")
        .and_then(|p| p.get(name))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("no metadata for {name}"))?;
    let minified = v.get("minified").and_then(Value::as_str).is_some();
    let versions: Vec<Map<String, Value>> = if minified {
        expand_minified(entries)
    } else {
        entries
            .iter()
            .filter_map(|e| e.as_object().cloned())
            .collect()
    };
    let mut info = PackageInfo {
        name: name.to_string(),
        ..PackageInfo::default()
    };
    let mut parsed: Vec<(Version, String)> = Vec::new();
    for entry in &versions {
        let s = |k: &str| entry.get(k).and_then(Value::as_str);
        if let Some(text) = s("version") {
            if let Some(v) = Version::parse(text) {
                parsed.push((v, text.to_string()));
            }
        }
    }
    // Descriptive fields from the newest version.
    if let Some(newest) = versions.first() {
        let s = |k: &str| newest.get(k).and_then(Value::as_str).map(str::to_string);
        info.description = s("description").unwrap_or_default();
        info.homepage = s("homepage").filter(|h| !h.is_empty());
        info.repository = newest
            .get("source")
            .and_then(|s| s.get("url"))
            .and_then(Value::as_str)
            .map(|u| u.trim_end_matches(".git").to_string());
        info.abandoned = match newest.get("abandoned") {
            Some(Value::String(r)) => Some(r.clone()),
            Some(Value::Bool(true)) => Some(String::new()),
            _ => None,
        };
    }
    parsed.sort_by(|a, b| b.0.cmp(&a.0));
    parsed.dedup_by(|a, b| a.1 == b.1);
    info.versions = parsed.into_iter().map(|(_, t)| t).collect();
    Ok(info)
}

/// A package the search finds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub favers: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default)]
    pub abandoned: bool,
}

/// Parse `search.json`.
pub fn parse_search(json: &str) -> Result<Vec<SearchHit>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("unexpected response: {e}"))?;
    let results = v
        .get("results")
        .and_then(Value::as_array)
        .ok_or("unexpected response: no results")?;
    Ok(results
        .iter()
        .filter_map(|r| {
            let s = |k: &str| r.get(k).and_then(Value::as_str).map(str::to_string);
            // Virtual packages have no repository; they cannot be required by name.
            Some(SearchHit {
                name: s("name")?,
                description: s("description").unwrap_or_default(),
                downloads: r.get("downloads").and_then(Value::as_u64).unwrap_or(0),
                favers: r.get("favers").and_then(Value::as_u64).unwrap_or(0),
                repository: s("repository").filter(|u| !u.is_empty()),
                abandoned: r.get("abandoned").is_some_and(|a| a != &Value::Bool(false)),
            })
        })
        .collect())
}

/// The metadata URL of a package (stable releases and pre-releases; `~dev` has branches).
pub fn p2_url(base: &str, name: &str) -> String {
    format!("{}/p2/{name}.json", base.trim_end_matches('/'))
}

/// The search URL, with the query percent-encoded.
pub fn search_url(base: &str, query: &str, per_page: usize) -> String {
    let mut q = String::new();
    for b in query.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                q.push(b as char)
            }
            b' ' => q.push('+'),
            _ => q.push_str(&format!("%{b:02X}")),
        }
    }
    format!(
        "{}/search.json?q={q}&per_page={per_page}",
        base.trim_end_matches('/')
    )
}

#[cfg(test)]
pub(crate) mod fixtures {
    /// A minified p2 response like Packagist's (trimmed).
    pub const LARAVEL_P2: &str = r#"{
        "minified": "composer/2.0",
        "packages": {"laravel/framework": [
            {"name": "laravel/framework", "description": "The Laravel Framework.", "homepage": "https://laravel.com",
             "version": "v12.3.0", "version_normalized": "12.3.0.0",
             "source": {"type": "git", "url": "https://github.com/laravel/framework.git"}, "require": {"php": "^8.2"}},
            {"version": "v12.2.0", "version_normalized": "12.2.0.0"},
            {"version": "v13.0.0-beta1", "version_normalized": "13.0.0.0-beta1"},
            {"version": "v12.1.0", "version_normalized": "12.1.0.0", "homepage": "__unset"},
            {"version": "v11.44.0", "version_normalized": "11.44.0.0"}
        ]}
    }"#;

    pub const SEARCH: &str = r#"{"results": [
        {"name": "laravel/framework", "description": "The Laravel Framework.", "url": "https://packagist.org/packages/laravel/framework",
         "repository": "https://github.com/laravel/framework", "downloads": 400000000, "favers": 33000},
        {"name": "laravel/sanctum", "description": "Laravel Sanctum provides a featherweight authentication system for SPAs and simple APIs.",
         "repository": "https://github.com/laravel/sanctum", "downloads": 100000000, "favers": 2800, "abandoned": false},
        {"name": "old/thing", "description": "", "repository": "", "downloads": 3, "favers": 0, "abandoned": "new/thing"}
    ], "total": 3}"#;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_minified_metadata() {
        let info = parse_p2("laravel/framework", fixtures::LARAVEL_P2).unwrap();
        assert_eq!(
            info.versions,
            ["v13.0.0-beta1", "v12.3.0", "v12.2.0", "v12.1.0", "v11.44.0"]
        );
        assert_eq!(info.description, "The Laravel Framework.");
        assert_eq!(info.homepage.as_deref(), Some("https://laravel.com"));
        assert_eq!(
            info.repository.as_deref(),
            Some("https://github.com/laravel/framework")
        );
        assert_eq!(info.latest().unwrap().original(), "v12.3.0");
        assert_eq!(info.suggested_constraint().as_deref(), Some("^12.3"));

        let raw: Value = serde_json::from_str(fixtures::LARAVEL_P2).unwrap();
        let list = raw["packages"]["laravel/framework"].as_array().unwrap();
        let expanded = expand_minified(list);
        assert_eq!(expanded[1]["description"], "The Laravel Framework.");
        assert!(!expanded[3].contains_key("homepage"));
        assert!(parse_p2("x/y", fixtures::LARAVEL_P2).is_err());
        assert!(parse_p2("x/y", "<html>").is_err());
    }

    #[test]
    fn parses_search_results() {
        let hits = parse_search(fixtures::SEARCH).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[1].name, "laravel/sanctum");
        assert!(!hits[1].abandoned && hits[2].abandoned);
        assert_eq!(hits[2].repository, None);
    }

    #[test]
    fn builds_urls() {
        assert_eq!(
            p2_url("https://repo.packagist.org/", "laravel/framework"),
            "https://repo.packagist.org/p2/laravel/framework.json"
        );
        assert_eq!(
            search_url(SEARCH_URL, "http client&x", 15),
            "https://packagist.org/search.json?q=http+client%26x&per_page=15"
        );
    }
}
