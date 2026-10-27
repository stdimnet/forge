//! The shared vulnerability model: what every audit tool's report becomes.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// The package ecosystem of a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Composer,
    Npm,
    Pypi,
    Cargo,
    Go,
    Maven,
}

impl Ecosystem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Composer => "Composer",
            Self::Npm => "npm",
            Self::Pypi => "PyPI",
            Self::Cargo => "crates.io",
            Self::Go => "Go",
            Self::Maven => "Maven",
        }
    }

    /// The name a package is matched by (PyPI names are normalized, Composer names are
    /// case-insensitive).
    pub fn key(self, name: &str) -> String {
        match self {
            Self::Pypi => pyforge_lang_python::normalize(name),
            Self::Composer => name.trim().to_ascii_lowercase(),
            _ => name.trim().to_string(),
        }
    }
}

/// How bad an advisory is, as the advisory databases rate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Moderate,
    Low,
    /// The database gives none (PyPI and Go advisories).
    #[default]
    Unknown,
}

impl Severity {
    /// `critical`, `high`, `moderate` / `medium`, `low` / `info` (any case).
    pub fn parse(s: &str) -> Severity {
        match s.trim().to_ascii_lowercase().as_str() {
            "critical" => Self::Critical,
            "high" => Self::High,
            "moderate" | "medium" => Self::Moderate,
            "low" | "info" | "informational" | "none" => Self::Low,
            _ => Self::Unknown,
        }
    }

    /// The CVSS qualitative rating of a base score.
    pub fn from_score(score: f64) -> Severity {
        match score {
            s if s >= 9.0 => Self::Critical,
            s if s >= 7.0 => Self::High,
            s if s >= 4.0 => Self::Moderate,
            s if s > 0.0 => Self::Low,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Moderate => "Moderate",
            Self::Low => "Low",
            Self::Unknown => "Unknown",
        }
    }

    /// Order for sorting and for the worst advisory of a package. An unrated advisory
    /// counts between low and moderate.
    pub fn rank(self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Unknown => 2,
            Self::Moderate => 3,
            Self::High => 4,
            Self::Critical => 5,
        }
    }

    /// The editor severity of the "Vulnerable Dependencies" problem: `error` (critical,
    /// high), `warning` (moderate, unknown) or `weak` (low).
    pub fn editor(self) -> &'static str {
        match self {
            Self::Critical | Self::High => "error",
            Self::Moderate | Self::Unknown => "warning",
            Self::Low => "weak",
        }
    }
}

/// One advisory affecting one installed package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vulnerability {
    pub ecosystem: Ecosystem,
    /// The package as the ecosystem names it (`laravel/framework`, `lodash`,
    /// `golang.org/x/net`, `org.apache.logging.log4j:log4j-core`).
    pub package: String,
    /// The installed (locked) version, when the tool reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
    /// The advisory id: a CVE when there is one, else the database's own (`GHSA-…`,
    /// `PYSEC-…`, `RUSTSEC-…`, `GO-…`).
    pub id: String,
    /// The other ids of the advisory.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub severity: Severity,
    pub title: String,
    /// The affected versions as the database writes them (`<4.17.21`, `>=2,<2.4.5`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected: Option<String>,
    /// The first version that fixes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The direct dependencies that pull a transitive package in (`vite` for `esbuild`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via: Vec<String>,
    /// govulncheck: the project's code calls the vulnerable symbol.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reachable: bool,
}

impl Vulnerability {
    pub fn new(ecosystem: Ecosystem, package: &str, id: &str, title: &str) -> Self {
        Vulnerability {
            ecosystem,
            package: package.to_string(),
            installed: None,
            id: id.to_string(),
            aliases: Vec::new(),
            severity: Severity::Unknown,
            title: title.to_string(),
            affected: None,
            fixed: None,
            url: None,
            via: Vec::new(),
            reachable: false,
        }
    }

    /// Make a CVE among the aliases the id (and keep the database's id as an alias).
    pub fn prefer_cve(mut self) -> Self {
        if !self.id.starts_with("CVE-") {
            if let Some(k) = self.aliases.iter().position(|a| a.starts_with("CVE-")) {
                let cve = self.aliases.remove(k);
                let own = std::mem::replace(&mut self.id, cve);
                if !own.is_empty() {
                    self.aliases.insert(0, own);
                }
            }
        }
        self.aliases.retain(|a| !a.is_empty());
        self.aliases.dedup();
        self
    }
}

/// What a package is besides vulnerable: abandoned (Composer), unmaintained or yanked
/// (cargo audit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeKind {
    Abandoned,
    Unmaintained,
    Yanked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub ecosystem: Ecosystem,
    pub package: String,
    pub kind: NoticeKind,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// The parsed report of one tool run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Findings {
    pub vulnerabilities: Vec<Vulnerability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<Notice>,
}

impl Findings {
    /// Sorted (worst first, then by package) and without duplicates (one advisory
    /// reported for several paths).
    pub fn normalized(mut self) -> Self {
        self.vulnerabilities.sort_by(|a, b| {
            b.severity
                .rank()
                .cmp(&a.severity.rank())
                .then_with(|| a.package.cmp(&b.package))
                .then_with(|| a.id.cmp(&b.id))
        });
        let mut out: Vec<Vulnerability> = Vec::new();
        for v in self.vulnerabilities {
            match out
                .iter_mut()
                .find(|o| o.package == v.package && o.id == v.id && o.installed == v.installed)
            {
                Some(o) => {
                    for d in v.via {
                        if !o.via.contains(&d) {
                            o.via.push(d);
                        }
                    }
                    o.reachable |= v.reachable;
                }
                None => out.push(v),
            }
        }
        self.vulnerabilities = out;
        self.notices
            .dedup_by(|a, b| a.package == b.package && a.kind == b.kind);
        self
    }
}

/// Compare two versions numerically part by part (`1.10.0` > `1.9.2`; a leading `v` and
/// pre-release suffixes are ignored for the numbers, a pre-release sorts first).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (Vec<u64>, bool) {
        let v = v.trim().trim_start_matches(['v', 'V', '=']);
        let (core, pre) = match v.find(['-', '+']) {
            Some(i) => (&v[..i], v[i..].starts_with('-')),
            None => (v, false),
        };
        let nums = core
            .split('.')
            .map(|p| {
                p.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect();
        (nums, pre)
    }
    let (x, xp) = split(a);
    let (y, yp) = split(b);
    for k in 0..x.len().max(y.len()) {
        let c = x
            .get(k)
            .copied()
            .unwrap_or(0)
            .cmp(&y.get(k).copied().unwrap_or(0));
        if c != Ordering::Equal {
            return c;
        }
    }
    yp.cmp(&xp)
}

/// The first fixed version of an affected range: the bound of its last `<` (`>=2,<2.4.5`
/// → `2.4.5`, `<4.17.21` → `4.17.21`); `None` for open ranges (`*`, `<=1.2.3`, `>=0`).
pub fn fixed_from_range(range: &str) -> Option<String> {
    // Alternatives (`<1.9.1|>=2,<2.4.5`, `<1.9.1 || >=2 <2.4.5`): the highest bound.
    let mut best: Option<String> = None;
    for alt in range.split("||").flat_map(|a| a.split('|')) {
        let alt = alt.trim();
        let i = alt.rfind('<')?;
        if alt[i + 1..].starts_with('=') {
            return None;
        }
        let v: String = alt[i + 1..]
            .trim()
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != ',')
            .collect();
        if v.is_empty() {
            return None;
        }
        if best
            .as_deref()
            .is_none_or(|b| compare_versions(&v, b) == Ordering::Greater)
        {
            best = Some(v);
        }
    }
    best
}

/// The lowest version of a `patched` requirement list (`[">=0.9.7"]`, `["^0.8.5",
/// ">=0.9.1"]`) above `installed`, else the highest one.
pub fn fixed_from_patched(patched: &[String], installed: Option<&str>) -> Option<String> {
    let mut versions: Vec<String> = patched
        .iter()
        .map(|p| {
            p.split(',')
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches(['>', '=', '^', '~', ' '])
                .to_string()
        })
        .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    versions.sort_by(|a, b| compare_versions(a, b));
    match installed {
        Some(i) => versions
            .iter()
            .find(|v| compare_versions(v, i) == Ordering::Greater)
            .or(versions.last())
            .cloned(),
        None => versions.first().cloned(),
    }
}

/// The highest of several fixed versions: the one that fixes them all.
pub fn max_version<'a>(versions: impl IntoIterator<Item = &'a str>) -> Option<String> {
    versions
        .into_iter()
        .max_by(|a, b| compare_versions(a, b))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_severities() {
        assert_eq!(Severity::parse("MEDIUM"), Severity::Moderate);
        assert_eq!(Severity::parse("whatever"), Severity::Unknown);
        assert_eq!(Severity::from_score(9.8), Severity::Critical);
        assert_eq!(Severity::from_score(5.3), Severity::Moderate);
        assert_eq!(Severity::High.editor(), "error");
        assert_eq!(Severity::Low.editor(), "weak");
        assert!(Severity::Unknown.rank() > Severity::Low.rank());
    }

    #[test]
    fn compares_versions() {
        assert_eq!(compare_versions("1.10.0", "1.9.2"), Ordering::Greater);
        assert_eq!(compare_versions("v0.23.0", "0.23"), Ordering::Equal);
        assert_eq!(compare_versions("2.0.0-beta.1", "2.0.0"), Ordering::Less);
    }

    #[test]
    fn finds_fixed_versions() {
        assert_eq!(fixed_from_range("<4.17.21").as_deref(), Some("4.17.21"));
        assert_eq!(fixed_from_range(">=2,<2.4.5").as_deref(), Some("2.4.5"));
        assert_eq!(
            fixed_from_range("<1.9.1|>=2,<2.4.5").as_deref(),
            Some("2.4.5")
        );
        assert_eq!(fixed_from_range(">=7.0.0 <7.5.2").as_deref(), Some("7.5.2"));
        assert_eq!(fixed_from_range("<=1.2.3"), None);
        assert_eq!(fixed_from_range("*"), None);
        let p = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            fixed_from_patched(&p(&["^0.8.5", ">=0.9.1"]), Some("0.9.0")).as_deref(),
            Some("0.9.1")
        );
        assert_eq!(
            fixed_from_patched(&p(&["^0.8.5", ">=0.9.1"]), Some("0.8.1")).as_deref(),
            Some("0.8.5")
        );
        assert_eq!(fixed_from_patched(&[], None), None);
        assert_eq!(max_version(["1.2.0", "1.10.0"]).as_deref(), Some("1.10.0"));
    }

    #[test]
    fn prefers_cves_and_merges_paths() {
        let mut v = Vulnerability::new(Ecosystem::Npm, "esbuild", "GHSA-67mh-4wv8-2f99", "x");
        v.aliases = vec!["CVE-2025-1".into()];
        let v = v.prefer_cve();
        assert_eq!(v.id, "CVE-2025-1");
        assert_eq!(v.aliases, ["GHSA-67mh-4wv8-2f99"]);
        let mut a = v.clone();
        a.via = vec!["vite".into()];
        let mut b = v;
        b.via = vec!["vitest".into()];
        let f = Findings {
            vulnerabilities: vec![a, b],
            notices: vec![],
        }
        .normalized();
        assert_eq!(f.vulnerabilities.len(), 1);
        assert_eq!(f.vulnerabilities[0].via, ["vite", "vitest"]);
    }
}
