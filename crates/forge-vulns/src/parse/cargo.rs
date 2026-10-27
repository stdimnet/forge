//! `cargo audit --json` (RustSec): `vulnerabilities.list` plus the `warnings`
//! (unmaintained, unsound, yanked crates). Severities come from the CVSS vector.

use serde_json::Value;

use crate::cvss;
use crate::model::{fixed_from_patched, Ecosystem, Findings, Notice, NoticeKind, Vulnerability};

use super::{document, first_sentence, s, strings};

pub fn parse(out: &str) -> Result<Findings, String> {
    let v = document(out, "cargo audit")?;
    let mut findings = Findings::default();
    for item in v
        .pointer("/vulnerabilities/list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(a) = item.get("advisory") else {
            continue;
        };
        let package = s(a, "package")
            .or_else(|| {
                item.pointer("/package/name")
                    .and_then(|x| x.as_str().map(str::to_string))
            })
            .unwrap_or_default();
        let id = s(a, "id").unwrap_or_default();
        let title = s(a, "title")
            .unwrap_or_else(|| first_sentence(&s(a, "description").unwrap_or_default()));
        let mut vuln = Vulnerability::new(Ecosystem::Cargo, &package, &id, &title);
        vuln.installed = item
            .pointer("/package/version")
            .and_then(Value::as_str)
            .map(str::to_string);
        vuln.aliases = strings(a, "aliases");
        vuln.severity = s(a, "cvss").map(|c| cvss::severity(&c)).unwrap_or_default();
        let patched = item
            .pointer("/versions/patched")
            .and_then(Value::as_array)
            .map(|p| {
                p.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        vuln.affected = (!patched.is_empty()).then(|| format!("not {}", patched.join(", ")));
        vuln.fixed = fixed_from_patched(&patched, vuln.installed.as_deref());
        vuln.url = Some(format!("https://rustsec.org/advisories/{id}"));
        findings.vulnerabilities.push(vuln.prefer_cve());
    }
    if let Some(warnings) = v.get("warnings").and_then(Value::as_object) {
        for (kind, list) in warnings {
            let kind = match kind.as_str() {
                "unmaintained" => NoticeKind::Unmaintained,
                "yanked" => NoticeKind::Yanked,
                _ => continue,
            };
            for w in list.as_array().into_iter().flatten() {
                let Some(name) = w.pointer("/package/name").and_then(Value::as_str) else {
                    continue;
                };
                let version = w
                    .pointer("/package/version")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let advisory = w.get("advisory");
                let message = match (kind, advisory.and_then(|a| s(a, "title"))) {
                    (NoticeKind::Yanked, _) => {
                        format!("{name} {version} was yanked from crates.io")
                    }
                    (_, Some(t)) => format!("{name} is unmaintained: {t}"),
                    _ => format!("{name} is unmaintained"),
                };
                findings.notices.push(Notice {
                    ecosystem: Ecosystem::Cargo,
                    package: name.to_string(),
                    kind,
                    message,
                    replacement: None,
                    url: advisory
                        .and_then(|a| s(a, "id"))
                        .map(|id| format!("https://rustsec.org/advisories/{id}")),
                });
            }
        }
    }
    Ok(findings)
}
