//! PyPI's JSON API (`/pypi/<name>/json`) and search page (`/search/?q=`), parsed into what
//! the Python Packages tool window and completion show.

use pyforge_lang_python::completion::sort_versions;
use pyforge_lang_python::pep508::{is_name, normalize};
use serde::{Deserialize, Serialize};

/// The default index.
pub const PYPI_URL: &str = "https://pypi.org";

/// A package on PyPI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PackageInfo {
    pub name: String,
    /// The latest release.
    pub version: String,
    pub summary: String,
    pub license: String,
    pub requires_python: String,
    pub author: String,
    pub home_page: String,
    /// All releases, newest first (final releases before pre-releases), yanked ones left out.
    pub versions: Vec<String>,
}

/// One search hit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchHit {
    pub name: String,
    pub version: String,
    pub summary: String,
}

/// `<index>/pypi/<name>/json`.
pub fn json_url(index: &str, name: &str) -> String {
    format!(
        "{}/pypi/{}/json",
        index.trim_end_matches('/'),
        normalize(name)
    )
}

/// `<index>/search/?q=<query>`.
pub fn search_url(index: &str, query: &str) -> String {
    let q: String = query
        .trim()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("{}/search/?q={q}", index.trim_end_matches('/'))
}

/// Parse the JSON API's answer.
pub fn parse_json(text: &str) -> Result<PackageInfo, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("unexpected PyPI response: {e}"))?;
    let info = v.get("info").ok_or("unexpected PyPI response: no info")?;
    let s = |k: &str| {
        info.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let home_page = match s("home_page") {
        h if !h.is_empty() => h,
        _ => info
            .get("project_urls")
            .and_then(|u| u.as_object())
            .and_then(|u| {
                ["Homepage", "Home", "Source", "Repository", "Documentation"]
                    .iter()
                    .find_map(|k| u.get(*k).and_then(|x| x.as_str()))
            })
            .unwrap_or("")
            .to_string(),
    };
    let versions: Vec<String> = v
        .get("releases")
        .and_then(|r| r.as_object())
        .map(|r| {
            r.iter()
                .filter(|(_, files)| {
                    // A release whose files are all yanked is not offered.
                    let files = files.as_array().map(Vec::as_slice).unwrap_or(&[]);
                    files.is_empty()
                        || files
                            .iter()
                            .any(|f| f.get("yanked").and_then(|y| y.as_bool()) != Some(true))
                })
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    // License: the short field, else the classifier.
    let mut license = s("license");
    if license.is_empty() || license.len() > 60 {
        license = info
            .get("classifiers")
            .and_then(|c| c.as_array())
            .and_then(|c| {
                c.iter().filter_map(|x| x.as_str()).find_map(|x| {
                    x.strip_prefix("License :: OSI Approved :: ")
                        .or_else(|| x.strip_prefix("License :: "))
                        .map(String::from)
                })
            })
            .or_else(|| Some(s("license_expression")).filter(|l| !l.is_empty()))
            .unwrap_or_default();
    }
    Ok(PackageInfo {
        name: s("name"),
        version: s("version"),
        summary: s("summary"),
        license,
        requires_python: s("requires_python"),
        author: s("author"),
        home_page,
        versions: sort_versions(&versions),
    })
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
}

/// The text of the first element with `class` after `from`, and where it ends.
fn class_text(html: &str, from: usize, class: &str, until: usize) -> Option<(String, usize)> {
    let at = html[from..until].find(&format!("class=\"{class}\""))? + from;
    let open_end = html[at..until].find('>')? + at + 1;
    let close = html[open_end..until].find("</")? + open_end;
    Some((decode_entities(html[open_end..close].trim()), close))
}

/// Parse PyPI's search page (`package-snippet` entries).
pub fn parse_search(html: &str) -> Vec<SearchHit> {
    let mut out = Vec::new();
    let marker = "class=\"package-snippet\"";
    let starts: Vec<usize> = html.match_indices(marker).map(|(i, _)| i).collect();
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(html.len());
        let Some((name, _)) = class_text(html, start, "package-snippet__name", end) else {
            continue;
        };
        if !is_name(&name) {
            continue;
        }
        let version = class_text(html, start, "package-snippet__version", end)
            .map(|(v, _)| v)
            .unwrap_or_default();
        let summary = class_text(html, start, "package-snippet__description", end)
            .map(|(v, _)| v)
            .unwrap_or_default();
        out.push(SearchHit {
            name,
            version,
            summary,
        });
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const REQUESTS_JSON: &str = r#"{"info": {"name": "requests", "version": "2.32.3", "summary": "Python HTTP for Humans.", "license": "Apache-2.0", "requires_python": ">=3.8", "author": "Kenneth Reitz", "home_page": "https://requests.readthedocs.io", "classifiers": []},
"releases": {"2.31.0": [{"yanked": false}], "2.32.0": [{"yanked": true}], "2.32.3": [{"yanked": false}], "3.0.0b1": [], "2.4.0": [{}]}}"#;

    pub const SEARCH_HTML: &str = r#"<ul><li><a class="package-snippet" href="/project/requests/">
<h3 class="package-snippet__title"><span class="package-snippet__name">requests</span>
<span class="package-snippet__version">2.32.3</span></h3>
<p class="package-snippet__description">Python HTTP for Humans.</p></a></li>
<li><a class="package-snippet" href="/project/requests-oauthlib/"><h3><span class="package-snippet__name">requests-oauthlib</span>
<span class="package-snippet__version">2.0.0</span></h3><p class="package-snippet__description">OAuthlib authentication support for Requests &amp; more.</p></a></li></ul>"#;

    #[test]
    fn parses_package_json() {
        let p = parse_json(REQUESTS_JSON).unwrap();
        assert_eq!(p.name, "requests");
        assert_eq!(p.version, "2.32.3");
        assert_eq!(p.license, "Apache-2.0");
        assert_eq!(p.versions, vec!["2.32.3", "2.31.0", "2.4.0", "3.0.0b1"]);
        assert!(parse_json("{}").is_err());
        let mit = parse_json(r#"{"info": {"name": "x", "version": "1", "license": "", "classifiers": ["License :: OSI Approved :: MIT License"], "project_urls": {"Source": "https://github.com/x/x"}}}"#).unwrap();
        assert_eq!(mit.license, "MIT License");
        assert_eq!(mit.home_page, "https://github.com/x/x");
    }

    #[test]
    fn parses_search_results() {
        let hits = parse_search(SEARCH_HTML);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[1].name, "requests-oauthlib");
        assert_eq!(
            hits[1].summary,
            "OAuthlib authentication support for Requests & more."
        );
        assert_eq!(
            search_url("https://pypi.org/", "http client"),
            "https://pypi.org/search/?q=http+client"
        );
        assert_eq!(
            json_url("https://pypi.org", "Django_REST"),
            "https://pypi.org/pypi/django-rest/json"
        );
    }
}
