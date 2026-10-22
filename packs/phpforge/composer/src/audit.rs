//! `composer audit --format=json`: security advisories of the installed packages and the
//! abandoned ones, as problems in composer.json and composer.lock and as rows of the
//! "Vulnerable Dependencies" tab.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::manifest::{self, Offsets, Position};

/// A security advisory affecting an installed package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Advisory {
    /// Packagist's id (`PKSA-…`).
    pub advisory_id: String,
    pub package_name: String,
    pub affected_versions: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cve: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_at: Option<String>,
    /// Ids of the advisory at its sources (`GHSA-…`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
}

/// An abandoned package and its suggested replacement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Abandoned {
    pub package_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub advisories: Vec<Advisory>,
    pub abandoned: Vec<Abandoned>,
}

fn advisory(v: &Value, package: &str) -> Option<Advisory> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Some(Advisory {
        advisory_id: s("advisoryId").unwrap_or_default(),
        package_name: s("packageName").unwrap_or_else(|| package.to_string()),
        affected_versions: s("affectedVersions").unwrap_or_default(),
        title: s("title")?,
        cve: s("cve").filter(|c| !c.is_empty()),
        link: s("link").filter(|c| !c.is_empty()),
        severity: s("severity").filter(|c| !c.is_empty()),
        reported_at: s("reportedAt"),
        sources: v
            .get("sources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|s| {
                s.get("remoteId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect(),
    })
}

/// Parse the JSON report. Composer writes the advisories of a package as an array, or as
/// an object keyed by index when some were filtered out.
pub fn parse(json: &str) -> Result<AuditReport, String> {
    // Composer may print notices before the JSON document.
    let start = json.find('{').ok_or("composer audit printed no report")?;
    let v: Value = serde_json::from_str(&json[start..])
        .map_err(|e| format!("cannot read the composer audit report: {e}"))?;
    let mut report = AuditReport::default();
    if let Some(map) = v.get("advisories").and_then(Value::as_object) {
        for (package, list) in map {
            let items: Vec<&Value> = match list {
                Value::Array(a) => a.iter().collect(),
                Value::Object(o) => o.values().collect(),
                _ => Vec::new(),
            };
            report
                .advisories
                .extend(items.into_iter().filter_map(|a| advisory(a, package)));
        }
    }
    if let Some(map) = v.get("abandoned").and_then(Value::as_object) {
        for (package, replacement) in map {
            report.abandoned.push(Abandoned {
                package_name: package.clone(),
                replacement: replacement.as_str().map(str::to_string),
            });
        }
    }
    Ok(report)
}

/// A problem for a file that may not be open: line/column positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProblem {
    pub start: Position,
    pub end: Position,
    pub message: String,
    /// `error` (advisory) or `warning` (abandoned).
    pub severity: String,
    pub package: String,
}

pub fn advisory_message(a: &Advisory) -> String {
    let id = a.cve.as_deref().unwrap_or(&a.advisory_id);
    format!(
        "{} is vulnerable: {} ({id}, affected versions: {})",
        a.package_name, a.title, a.affected_versions
    )
}

pub fn abandoned_message(a: &Abandoned) -> String {
    match a.replacement.as_deref() {
        Some(r) if !r.is_empty() => {
            format!("Package {} is abandoned, use {r} instead", a.package_name)
        }
        _ => format!(
            "Package {} is abandoned and no replacement was suggested",
            a.package_name
        ),
    }
}

/// Problems of the report in composer.json (at the requirement, when the package is a
/// direct dependency) and in composer.lock (at the package's entry).
pub fn problems(
    report: &AuditReport,
    composer_json: Option<&str>,
    composer_lock: Option<&str>,
) -> (Vec<FileProblem>, Vec<FileProblem>) {
    let entries: Vec<(&str, String, &str)> = report
        .advisories
        .iter()
        .map(|a| (a.package_name.as_str(), advisory_message(a), "error"))
        .chain(
            report
                .abandoned
                .iter()
                .map(|a| (a.package_name.as_str(), abandoned_message(a), "warning")),
        )
        .collect();
    let mut in_json = Vec::new();
    if let Some(text) = composer_json {
        let o = Offsets::new(text);
        let reqs = manifest::requirements(text);
        for (name, message, severity) in &entries {
            if let Some(r) = reqs.iter().find(|r| r.name == *name) {
                in_json.push(FileProblem {
                    start: o.position(r.key.0),
                    end: o.position(r.key.1),
                    message: message.clone(),
                    severity: severity.to_string(),
                    package: name.to_string(),
                });
            }
        }
    }
    let mut in_lock = Vec::new();
    if let Some(text) = composer_lock {
        let o = Offsets::new(text);
        for (name, message, severity) in &entries {
            if let Some((s, e)) = manifest::lock_entry(text, name) {
                in_lock.push(FileProblem {
                    start: o.position(s),
                    end: o.position(e),
                    message: message.clone(),
                    severity: severity.to_string(),
                    package: name.to_string(),
                });
            }
        }
    }
    (in_json, in_lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = r#"Some notice
{
    "advisories": {
        "guzzlehttp/psr7": [
            {"advisoryId": "PKSA-8qx3-n5y5-vvnd", "packageName": "guzzlehttp/psr7", "affectedVersions": ">=2,<2.4.5",
             "title": "Improper header validation", "cve": "CVE-2023-29197", "link": "https://github.com/advisories/GHSA-wxmh-65f7-jcvw",
             "reportedAt": "2023-04-17T16:00:00+00:00", "sources": [{"name": "GitHub", "remoteId": "GHSA-wxmh-65f7-jcvw"}], "severity": "medium"}
        ],
        "symfony/http-kernel": {"1": {"advisoryId": "PKSA-x", "affectedVersions": "<6.4.1", "title": "Cache poisoning", "cve": null}}
    },
    "abandoned": {"swiftmailer/swiftmailer": "symfony/mailer", "fzaninotto/faker": null}
}"#;

    #[test]
    fn parses_the_report() {
        let r = parse(REPORT).unwrap();
        assert_eq!(r.advisories.len(), 2);
        let psr7 = r
            .advisories
            .iter()
            .find(|a| a.package_name == "guzzlehttp/psr7")
            .unwrap();
        assert_eq!(psr7.cve.as_deref(), Some("CVE-2023-29197"));
        assert_eq!(psr7.sources, ["GHSA-wxmh-65f7-jcvw"]);
        let kernel = r
            .advisories
            .iter()
            .find(|a| a.package_name == "symfony/http-kernel")
            .unwrap();
        assert_eq!(kernel.cve, None);
        assert_eq!(r.abandoned.len(), 2);
        assert!(parse("no json here").is_err());
        assert_eq!(
            advisory_message(psr7),
            "guzzlehttp/psr7 is vulnerable: Improper header validation (CVE-2023-29197, affected versions: >=2,<2.4.5)"
        );
    }

    #[test]
    fn places_problems_in_both_files() {
        let r = parse(REPORT).unwrap();
        let json = "{\n  \"require\": {\n    \"guzzlehttp/psr7\": \"^2.0\"\n  }\n}";
        let lock = "{\n  \"packages\": [\n    {\"name\": \"guzzlehttp/psr7\"},\n    {\"name\": \"symfony/http-kernel\"},\n    {\"name\": \"swiftmailer/swiftmailer\"}\n  ]\n}";
        let (in_json, in_lock) = problems(&r, Some(json), Some(lock));
        assert_eq!(in_json.len(), 1);
        assert_eq!(
            in_json[0].start,
            Position {
                line: 2,
                character: 4
            }
        );
        assert_eq!(in_lock.len(), 3);
        assert_eq!(in_lock[2].severity, "warning");
        assert_eq!(
            in_lock[2].message,
            "Package swiftmailer/swiftmailer is abandoned, use symfony/mailer instead"
        );
    }
}
