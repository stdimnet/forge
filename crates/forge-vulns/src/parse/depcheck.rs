//! The JSON report of OWASP dependency-check (`dependency-check-report.json`, written by
//! its Maven / Gradle plugin or CLI with `--format JSON`). Forge only reads the report the
//! build wrote; dependencies are named by their Maven coordinates from the package URL
//! (`pkg:maven/org.apache.logging.log4j/log4j-core@2.14.1`).

use serde_json::Value;

use crate::model::{Ecosystem, Findings, Severity, Vulnerability};

use super::{document, first_sentence, s};

/// `group:artifact` and version of a Maven package URL.
fn coordinates(purl: &str) -> Option<(String, String)> {
    let rest = purl.strip_prefix("pkg:maven/")?;
    let rest = rest.split(['?', '#']).next()?;
    let (path, version) = rest.rsplit_once('@')?;
    let (group, artifact) = path.rsplit_once('/')?;
    Some((format!("{group}:{artifact}"), version.to_string()))
}

pub fn parse(out: &str) -> Result<Findings, String> {
    let v = document(out, "dependency-check")?;
    let mut findings = Findings::default();
    for d in v
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(vulns) = d.get("vulnerabilities").and_then(Value::as_array) else {
            continue;
        };
        let coords = d
            .get("packages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| s(p, "id"))
            .find_map(|id| coordinates(&id));
        let (package, version) = match coords {
            Some((p, v)) => (p, Some(v)),
            None => (s(d, "fileName").unwrap_or_default(), None),
        };
        for x in vulns {
            let id = s(x, "name").unwrap_or_default();
            let description = s(x, "description").unwrap_or_default();
            let mut vuln = Vulnerability::new(
                Ecosystem::Maven,
                &package,
                &id,
                &first_sentence(&description),
            );
            vuln.installed = version.clone();
            vuln.severity = match s(x, "severity").map(|s| Severity::parse(&s)) {
                Some(sev) if sev != Severity::Unknown => sev,
                _ => x
                    .pointer("/cvssv3/baseScore")
                    .or_else(|| x.pointer("/cvssv2/score"))
                    .and_then(Value::as_f64)
                    .map(Severity::from_score)
                    .unwrap_or_default(),
            };
            let matched = x
                .get("vulnerableSoftware")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|w| w.get("software"))
                .find(|w| {
                    s(w, "versionEndExcluding").is_some() || s(w, "versionEndIncluding").is_some()
                });
            if let Some(w) = matched {
                let lo = s(w, "versionStartIncluding").map(|l| format!(">={l} "));
                vuln.affected = match (s(w, "versionEndExcluding"), s(w, "versionEndIncluding")) {
                    (Some(e), _) => Some(format!("{}<{e}", lo.unwrap_or_default())),
                    (_, Some(e)) => Some(format!("{}<={e}", lo.unwrap_or_default())),
                    _ => None,
                };
                vuln.fixed = s(w, "versionEndExcluding");
            }
            vuln.url = Some(if id.starts_with("CVE-") {
                format!("https://nvd.nist.gov/vuln/detail/{id}")
            } else {
                x.get("references")
                    .and_then(Value::as_array)
                    .and_then(|r| r.first())
                    .and_then(|r| s(r, "url"))
                    .unwrap_or_default()
            })
            .filter(|u| !u.is_empty());
            findings.vulnerabilities.push(vuln);
        }
    }
    Ok(findings)
}
