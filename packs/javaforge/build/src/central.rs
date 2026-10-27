//! Maven Central: the search API (`search.maven.org/solrsearch/select`) for coordinate
//! completion and `maven-metadata.xml` of the repository for versions. Only URLs and
//! parsers live here (pure); [`crate::net`] downloads and caches them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::version;

pub const SEARCH_URL: &str = "https://search.maven.org";
pub const REPOSITORY_URL: &str = "https://repo1.maven.org/maven2";

/// An artifact the search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub group: String,
    pub artifact: String,
    #[serde(default)]
    pub latest_version: String,
}

/// The versions of an artifact, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Versions {
    pub versions: Vec<String>,
    /// The newest release (no alpha, beta, RC, milestone or snapshot).
    #[serde(default)]
    pub latest: Option<String>,
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'*' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn valid_part(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// The search query for what was typed: `com.google.gu` (free text), `com.google.guava:`
/// or `com.google.guava:gu` (the group's artifacts).
pub fn search_query(prefix: &str) -> Option<String> {
    let prefix = prefix.trim();
    match prefix.split_once(':') {
        Some((g, _)) if valid_part(g) => Some(format!("g:\"{g}\"")),
        Some(_) => None,
        None if prefix.len() >= 2
            && prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) =>
        {
            Some(prefix.to_string())
        }
        None => None,
    }
}

pub fn search_url(base: &str, query: &str, rows: usize) -> String {
    format!(
        "{}/solrsearch/select?q={}&rows={rows}&wt=json",
        base.trim_end_matches('/'),
        encode(query)
    )
}

/// Read a search answer.
pub fn parse_search(json: &str) -> Result<Vec<Artifact>, String> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected Maven Central answer: {e}"))?;
    let docs = v
        .pointer("/response/docs")
        .and_then(Value::as_array)
        .ok_or("unexpected Maven Central answer: no docs")?;
    Ok(docs
        .iter()
        .filter_map(|d| {
            Some(Artifact {
                group: d.get("g")?.as_str()?.to_string(),
                artifact: d.get("a")?.as_str()?.to_string(),
                latest_version: d
                    .get("latestVersion")
                    .or_else(|| d.get("v"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect())
}

/// `maven-metadata.xml` of `group:artifact`.
pub fn metadata_url(repository: &str, group: &str, artifact: &str) -> Option<String> {
    (valid_part(group) && valid_part(artifact)).then(|| {
        format!(
            "{}/{}/{artifact}/maven-metadata.xml",
            repository.trim_end_matches('/'),
            group.replace('.', "/")
        )
    })
}

/// Read `maven-metadata.xml`.
pub fn parse_metadata(xml: &str) -> Result<Versions, String> {
    let start = xml
        .find("<versions>")
        .ok_or("maven-metadata.xml lists no versions")?;
    let end = xml[start..]
        .find("</versions>")
        .map_or(xml.len(), |e| start + e);
    let mut versions: Vec<String> = Vec::new();
    let mut rest = &xml[start..end];
    while let Some(s) = rest.find("<version>") {
        let body = &rest[s + 9..];
        let Some(e) = body.find("</version>") else {
            break;
        };
        let v = body[..e].trim();
        if !v.is_empty() && !versions.iter().any(|x| x == v) {
            versions.push(v.to_string());
        }
        rest = &body[e..];
    }
    versions.sort_by(|a, b| version::compare(b, a));
    let latest = version::latest(versions.iter().map(String::as_str), true)
        .filter(|v| !version::is_prerelease(v))
        .map(str::to_string);
    Ok(Versions { versions, latest })
}

#[cfg(test)]
pub(crate) mod fixtures {
    pub const SEARCH: &str = include_str!("../tests/fixtures/central-search-guava.json");
    pub const GROUP: &str = include_str!("../tests/fixtures/central-search-junit-group.json");
    pub const METADATA: &str = include_str!("../tests/fixtures/guava-maven-metadata.xml");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_and_urls() {
        assert_eq!(search_query("guav").as_deref(), Some("guav"));
        assert_eq!(
            search_query("org.junit.jupiter:ju").as_deref(),
            Some("g:\"org.junit.jupiter\"")
        );
        assert_eq!(search_query("g"), None);
        assert_eq!(search_query("a b"), None);
        assert_eq!(
            search_url("https://search.maven.org/", "g:\"org.junit\"", 20),
            "https://search.maven.org/solrsearch/select?q=g%3A%22org.junit%22&rows=20&wt=json"
        );
        assert_eq!(
            metadata_url(REPOSITORY_URL, "com.google.guava", "guava").as_deref(),
            Some("https://repo1.maven.org/maven2/com/google/guava/guava/maven-metadata.xml")
        );
        assert_eq!(metadata_url(REPOSITORY_URL, "../x", "y"), None);
    }

    #[test]
    fn parses_answers() {
        let hits = parse_search(fixtures::SEARCH).unwrap();
        assert_eq!(hits[0].artifact, "guava");
        assert_eq!(hits[0].latest_version, "33.3.1-jre");
        assert!(parse_search("{}").is_err());
        let group = parse_search(fixtures::GROUP).unwrap();
        assert!(group.iter().any(|a| a.artifact == "junit-jupiter-api"));
        let v = parse_metadata(fixtures::METADATA).unwrap();
        assert!(v.versions[0].starts_with("33.4.0-"));
        assert!(v.latest.as_deref().unwrap().starts_with("33.4.0-"));
        assert!(
            !v.versions.iter().any(|x| x == "33.5.0-rc1")
                || v.latest.as_deref() != Some("33.5.0-rc1")
        );
        assert!(v.versions.contains(&"32.0.0-jre".to_string()));
        assert!(parse_metadata("<metadata/>").is_err());
    }
}
