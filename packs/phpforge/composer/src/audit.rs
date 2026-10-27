//! `composer audit --format=json`: security advisories of the installed packages and the
//! abandoned ones. `forge-vulns` adapts the report to the shared vulnerability model and
//! places the problems in composer.json and composer.lock.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
            abandoned_message(
                r.abandoned
                    .iter()
                    .find(|a| a.package_name == "fzaninotto/faker")
                    .unwrap()
            ),
            "Package fzaninotto/faker is abandoned and no replacement was suggested"
        );
    }
}
