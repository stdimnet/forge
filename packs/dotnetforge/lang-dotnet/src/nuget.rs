//! NuGet v3 server API answers (nuget.org and other feeds): the service index, search
//! results and a package's versions (`PackageBaseAddress` flat container). The HTTP client
//! lives in `dotnetforge-sdk`; this module only reads and builds.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const NUGET_ORG: &str = "https://api.nuget.org/v3/index.json";

/// The resources Forge uses from a feed's service index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceIndex {
    pub search: Option<String>,
    /// `PackageBaseAddress/3.0.0` (flat container: versions and `.nupkg` files).
    pub package_base: Option<String>,
}

pub fn parse_service_index(json: &str) -> Result<ServiceIndex, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("service index: {e}"))?;
    let resources = v
        .get("resources")
        .and_then(Value::as_array)
        .ok_or("the service index has no resources")?;
    let find = |prefix: &str| -> Option<String> {
        resources
            .iter()
            .filter(|r| {
                r.get("@type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.starts_with(prefix))
            })
            .filter_map(|r| r.get("@id").and_then(Value::as_str))
            .map(|s| s.trim_end_matches('/').to_string())
            .next()
    };
    Ok(ServiceIndex {
        search: find("SearchQueryService"),
        package_base: find("PackageBaseAddress"),
    })
}

/// A package of a search result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NuGetPackage {
    pub id: String,
    /// The latest version (a prerelease only when prereleases were asked for).
    pub version: String,
    pub description: String,
    pub authors: String,
    pub total_downloads: u64,
    /// The id prefix is reserved by its owner (the ✓ on nuget.org).
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Versions, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    /// The latest version has known vulnerabilities.
    #[serde(default)]
    pub vulnerable: bool,
    /// The package is deprecated.
    #[serde(default)]
    pub deprecated: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub total_hits: u64,
    pub packages: Vec<NuGetPackage>,
}

fn text_or_list(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

fn opt_url(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn parse_search(json: &str) -> Result<SearchResult, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("search: {e}"))?;
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or("the search answer has no data")?;
    let packages = data
        .iter()
        .filter_map(|p| {
            let id = p.get("id")?.as_str()?.to_string();
            let tags = match p.get("tags") {
                Some(Value::Array(a)) => a
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                Some(Value::String(s)) => s.split_whitespace().map(str::to_string).collect(),
                _ => Vec::new(),
            };
            let versions = p
                .get("versions")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.get("version").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let description = match p.get("summary").and_then(Value::as_str) {
                Some(s) if !s.trim().is_empty() => s.to_string(),
                _ => p
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            };
            Some(NuGetPackage {
                version: p
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                description: description.trim().to_string(),
                authors: text_or_list(p.get("authors")),
                total_downloads: p.get("totalDownloads").and_then(Value::as_u64).unwrap_or(0),
                verified: p.get("verified").and_then(Value::as_bool).unwrap_or(false),
                icon_url: opt_url(p, "iconUrl"),
                project_url: opt_url(p, "projectUrl"),
                license_url: opt_url(p, "licenseUrl"),
                tags,
                versions,
                vulnerable: p
                    .get("vulnerabilities")
                    .and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty()),
                deprecated: p.get("deprecation").is_some_and(|d| !d.is_null()),
                id,
            })
        })
        .collect();
    Ok(SearchResult {
        total_hits: v.get("totalHits").and_then(Value::as_u64).unwrap_or(0),
        packages,
    })
}

/// `{ "versions": [...] }` of `<package base>/<id lowercase>/index.json`.
pub fn parse_versions(json: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("versions: {e}"))?;
    Ok(v.get("versions")
        .and_then(Value::as_array)
        .ok_or("no versions")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect())
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `<search>?q=…&skip=…&take=…&prerelease=…&semVerLevel=2.0.0`.
pub fn search_url(search: &str, query: &str, skip: u32, take: u32, prerelease: bool) -> String {
    format!(
        "{}?q={}&skip={skip}&take={take}&prerelease={prerelease}&semVerLevel=2.0.0",
        search.trim_end_matches('/'),
        encode(query.trim())
    )
}

/// `<package base>/<id>/index.json` (ids are lowercase in the flat container).
pub fn versions_url(package_base: &str, id: &str) -> String {
    format!(
        "{}/{}/index.json",
        package_base.trim_end_matches('/'),
        encode(&id.to_ascii_lowercase())
    )
}

/// The nuget.org page of a package.
pub fn gallery_url(id: &str, version: &str) -> String {
    if version.is_empty() {
        format!("https://www.nuget.org/packages/{id}")
    } else {
        format!("https://www.nuget.org/packages/{id}/{version}")
    }
}

/// `1234567` → `1.2M`, like the nuget.org gallery.
pub fn short_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}K", n as f64 / 1e3),
        1_000_000..=999_999_999 => format!("{:.1}M", n as f64 / 1e6),
        _ => format!("{:.1}B", n as f64 / 1e9),
    }
    .replace(".0", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn service_index_search_and_versions() {
        let idx = parse_service_index(&fixture("nuget-index.json")).unwrap();
        assert_eq!(
            idx.search.as_deref(),
            Some("https://azuresearch-usnc.nuget.org/query")
        );
        assert_eq!(
            idx.package_base.as_deref(),
            Some("https://api.nuget.org/v3-flatcontainer")
        );
        assert!(parse_service_index("{}").is_err());

        let r = parse_search(&fixture("nuget-search-serilog.json")).unwrap();
        assert_eq!(r.total_hits, 1784);
        assert_eq!(r.packages.len(), 3);
        let s = &r.packages[0];
        assert_eq!((s.id.as_str(), s.version.as_str()), ("Serilog", "4.1.0"));
        assert!(s.verified);
        assert_eq!(s.authors, "Serilog Contributors");
        assert_eq!(s.versions.last().unwrap(), "4.1.0");
        assert_eq!(r.packages[1].authors, "Microsoft, Serilog Contributors");
        assert!(r.packages[2].deprecated && r.packages[2].vulnerable);
        assert_eq!(r.packages[2].tags, vec!["serilog", "sinks"]);

        assert_eq!(
            parse_versions("{\"versions\":[\"1.0.0\",\"2.0.0-beta\"]}")
                .unwrap()
                .len(),
            2
        );
        assert!(parse_versions("{}").is_err());
    }

    #[test]
    fn urls_and_counts() {
        assert_eq!(
            search_url("https://s/query/", "serilog sinks", 0, 25, false),
            "https://s/query?q=serilog+sinks&skip=0&take=25&prerelease=false&semVerLevel=2.0.0"
        );
        assert_eq!(
            versions_url("https://f/", "Serilog.Sinks.Console"),
            "https://f/serilog.sinks.console/index.json"
        );
        assert_eq!(encode("a&b"), "a%26b");
        assert_eq!(short_count(1_234_567), "1.2M");
        assert_eq!(short_count(2_000), "2K");
        assert_eq!(short_count(999), "999");
        assert_eq!(
            gallery_url("X", "1.0"),
            "https://www.nuget.org/packages/X/1.0"
        );
    }
}
