//! The Gradle wrapper: `gradle/wrapper/gradle-wrapper.properties` (the distribution, its
//! version and `distributionSha256Sum`), Update Wrapper (a new `distributionUrl` and the
//! checksum services.gradle.org publishes) and the answers of services.gradle.org.

use serde::{Deserialize, Serialize};

/// Where the wrapper's properties live, relative to the build's root.
pub const PROPERTIES: &str = "gradle/wrapper/gradle-wrapper.properties";
/// The official distributions.
pub const DISTRIBUTIONS: &str = "https://services.gradle.org/distributions";
/// `versions/current` answers the newest release.
pub const SERVICES: &str = "https://services.gradle.org";

/// What `gradle-wrapper.properties` says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradleWrapper {
    /// `8.10.2` (empty when the URL names no version).
    pub version: String,
    pub distribution_url: String,
    /// `bin` or `all`.
    pub distribution_type: String,
    /// `distributionSha256Sum`, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Java properties: `key=value` / `key: value`, `\:` escapes, `#` / `!` comments.
fn properties(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let split = line
            .char_indices()
            .find(|&(i, c)| (c == '=' || c == ':') && !line[..i].ends_with('\\'));
        let Some((i, _)) = split else { continue };
        let key = line[..i].trim().replace("\\:", ":");
        let value = unescape(line[i + 1..].trim());
        out.push((key, value));
    }
    out
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `gradle-8.10.2-bin.zip` → (`8.10.2`, `bin`).
pub fn version_of_url(url: &str) -> Option<(String, String)> {
    let file = url.rsplit('/').next()?;
    let rest = file.strip_prefix("gradle-")?.strip_suffix(".zip")?;
    let (version, kind) = rest.rsplit_once('-')?;
    (!version.is_empty()).then(|| (version.to_string(), kind.to_string()))
}

/// Read `gradle-wrapper.properties`.
pub fn parse_properties(text: &str) -> Option<GradleWrapper> {
    let props = properties(text);
    let get = |k: &str| {
        props
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    let url = get("distributionUrl")?;
    let (version, kind) = version_of_url(&url).unwrap_or_default();
    Some(GradleWrapper {
        version,
        distribution_url: url,
        distribution_type: if kind.is_empty() { "bin".into() } else { kind },
        sha256: get("distributionSha256Sum")
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty()),
    })
}

/// The official download of a release.
pub fn distribution_url(version: &str, kind: &str) -> String {
    let kind = if kind == "all" { "all" } else { "bin" };
    format!("{DISTRIBUTIONS}/gradle-{version}-{kind}.zip")
}

/// Properties files escape `:` (`https\://…`), as Gradle writes them.
fn escape(v: &str) -> String {
    v.replace(':', "\\:")
}

/// The properties after Update Wrapper: the new `distributionUrl`, and the new
/// `distributionSha256Sum` (added when `sha256` is known, removed otherwise because the old
/// sum would fail). Other lines stay as they are.
pub fn update_properties(text: &str, url: &str, sha256: Option<&str>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut url_done = false;
    let mut sum_done = false;
    for line in text.lines() {
        let key = line.split(['=', ':']).next().unwrap_or("").trim();
        match key {
            "distributionUrl" => {
                out.push(format!("distributionUrl={}", escape(url)));
                url_done = true;
            }
            "distributionSha256Sum" => {
                if let Some(s) = sha256 {
                    out.push(format!("distributionSha256Sum={s}"));
                }
                sum_done = true;
            }
            _ => out.push(line.to_string()),
        }
    }
    if !url_done {
        out.push(format!("distributionUrl={}", escape(url)));
    }
    if let (false, Some(s)) = (sum_done, sha256) {
        // Right after the URL, like `gradle wrapper --gradle-distribution-sha256-sum`.
        let at = out
            .iter()
            .position(|l| l.starts_with("distributionUrl="))
            .map_or(out.len(), |i| i + 1);
        out.insert(at, format!("distributionSha256Sum={s}"));
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// The newest release according to `services.gradle.org/versions/current`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradleRelease {
    pub version: String,
    pub download_url: String,
    #[serde(default)]
    pub checksum_url: String,
}

pub fn parse_current(json: &str) -> Result<GradleRelease, String> {
    let r: GradleRelease = serde_json::from_str(json)
        .map_err(|e| format!("unexpected answer of services.gradle.org: {e}"))?;
    if r.version.is_empty() || r.download_url.is_empty() {
        return Err("services.gradle.org named no release".into());
    }
    Ok(r)
}

/// A `.sha256` file: the hex digest (optionally followed by the file name).
pub fn parse_sha256(text: &str) -> Option<String> {
    text.split_whitespace()
        .next()
        .map(str::to_ascii_lowercase)
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
}

/// How `distributionSha256Sum` compares with the checksum Gradle publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Checksum {
    /// The properties set no checksum: the wrapper does not verify the download.
    NotSet,
    Matches,
    /// The checksum differs from the official one: the wrapper refuses the download (or
    /// the URL points at a different distribution).
    Mismatch,
    /// The distribution is not an official one, so there is nothing to compare with.
    Unknown,
}

pub fn check(wrapper: &GradleWrapper, published: Option<&str>) -> Checksum {
    match (&wrapper.sha256, published) {
        (None, _) => Checksum::NotSet,
        (Some(_), None) => Checksum::Unknown,
        (Some(a), Some(b)) if a.eq_ignore_ascii_case(b.trim()) => Checksum::Matches,
        _ => Checksum::Mismatch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPS: &str = "distributionBase=GRADLE_USER_HOME\ndistributionPath=wrapper/dists\ndistributionSha256Sum=31C55713E40233A8303827CEB42CA48A47267A0AD4BAB9177123121E71524C26\ndistributionUrl=https\\://services.gradle.org/distributions/gradle-8.10.2-bin.zip\nnetworkTimeout=10000\n# comment\nzipStorePath=wrapper/dists\n";

    #[test]
    fn reads_the_wrapper_properties() {
        let w = parse_properties(PROPS).unwrap();
        assert_eq!(w.version, "8.10.2");
        assert_eq!(w.distribution_type, "bin");
        assert_eq!(
            w.distribution_url,
            "https://services.gradle.org/distributions/gradle-8.10.2-bin.zip"
        );
        assert_eq!(
            w.sha256.as_deref(),
            Some("31c55713e40233a8303827ceb42ca48a47267a0ad4bab9177123121e71524c26")
        );
        assert_eq!(
            version_of_url("https://x/gradle-8.11-rc-1-all.zip"),
            Some(("8.11-rc-1".into(), "all".into()))
        );
        assert!(parse_properties("networkTimeout=1").is_none());
    }

    #[test]
    fn updates_url_and_checksum() {
        let url = distribution_url("8.11", "bin");
        let sum = "a".repeat(64);
        let out = update_properties(PROPS, &url, Some(&sum));
        assert!(out.contains(
            "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.11-bin.zip\n"
        ));
        assert!(out.contains(&format!("distributionSha256Sum={sum}\n")));
        assert!(out.contains("# comment\n"));
        let w = parse_properties(&out).unwrap();
        assert_eq!(w.version, "8.11");
        // Without a known checksum the stale one goes.
        let out = update_properties(PROPS, &url, None);
        assert!(!out.contains("distributionSha256Sum"));
        // A file without a checksum gets one after the URL.
        let plain = "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.0-bin.zip\nzipStorePath=x\n";
        let out = update_properties(plain, &url, Some(&sum));
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[1].starts_with("distributionSha256Sum="));
    }

    #[test]
    fn services_answers_and_checks() {
        let r = parse_current(r#"{"version":"8.11","buildTime":"x","current":true,"downloadUrl":"https://services.gradle.org/distributions/gradle-8.11-bin.zip","checksumUrl":"https://services.gradle.org/distributions/gradle-8.11-bin.zip.sha256"}"#).unwrap();
        assert_eq!(r.version, "8.11");
        assert!(parse_current("{}").is_err());
        let sum = "31c55713e40233a8303827ceb42ca48a47267a0ad4bab9177123121e71524c26";
        assert_eq!(parse_sha256(&format!("{sum}\n")).as_deref(), Some(sum));
        assert_eq!(parse_sha256("nope"), None);
        let w = parse_properties(PROPS).unwrap();
        assert_eq!(check(&w, Some(sum)), Checksum::Matches);
        assert_eq!(check(&w, Some(&"0".repeat(64))), Checksum::Mismatch);
        assert_eq!(check(&w, None), Checksum::Unknown);
        let unset = GradleWrapper { sha256: None, ..w };
        assert_eq!(check(&unset, Some(sum)), Checksum::NotSet);
    }
}
