//! The npm family: `npm audit --json` (npm 7+, `auditReportVersion: 2`), the npm 6 format
//! that `pnpm audit --json` and Yarn 2/3's `yarn npm audit --json` still write
//! (`advisories` by id), Yarn 1's `yarn audit --json` (one `auditAdvisory` event per line)
//! and Yarn 4's `yarn npm audit --json` (one package tree per line).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::model::{fixed_from_range, Ecosystem, Findings, Severity, Vulnerability};

use super::{s, strings};

pub fn parse(out: &str) -> Result<Findings, String> {
    let trimmed = out.trim();
    if let Some(start) = trimmed.find('{') {
        if let Ok(v) = serde_json::from_str::<Value>(&trimmed[start..]) {
            if let Some(e) = v.get("error") {
                let msg = s(e, "summary")
                    .or_else(|| s(e, "message"))
                    .or_else(|| e.as_str().map(str::to_string))
                    .unwrap_or_else(|| "the audit failed".into());
                return Err(msg.lines().next().unwrap_or("").to_string());
            }
            if v.get("auditReportVersion").is_some() || v.get("vulnerabilities").is_some() {
                return Ok(report_v2(&v));
            }
            if let Some(adv) = v.get("advisories").and_then(Value::as_object) {
                return Ok(advisories(adv.values()));
            }
            if v.get("value").is_none() && v.get("type").is_none() {
                return Ok(Findings::default());
            }
        }
    }
    // One JSON document per line.
    let mut classic = Vec::new();
    let mut tree = Findings::default();
    let mut any = false;
    for line in trimmed.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        any = true;
        match v.get("type").and_then(Value::as_str) {
            Some("auditAdvisory") => {
                if let Some(a) = v.pointer("/data/advisory") {
                    classic.push(a.clone());
                }
            }
            Some("error") => {
                return Err(s(&v, "data").unwrap_or_else(|| "the audit failed".into()));
            }
            Some(_) => {}
            None => {
                if let Some(vuln) = berry_entry(&v) {
                    tree.vulnerabilities.push(vuln);
                }
            }
        }
    }
    if !any {
        return Err("the audit printed no JSON report".into());
    }
    let mut out = advisories(classic.iter());
    out.vulnerabilities.extend(tree.vulnerabilities);
    Ok(out)
}

fn ghsa_of(url: &str) -> Option<String> {
    url.rsplit('/')
        .next()
        .filter(|id| id.starts_with("GHSA-"))
        .map(str::to_string)
}

fn severity_of(v: &Value) -> Severity {
    let rated = s(v, "severity").map(|x| Severity::parse(&x));
    match rated {
        Some(r) if r != Severity::Unknown => r,
        _ => v
            .pointer("/cvss/score")
            .and_then(Value::as_f64)
            .map(Severity::from_score)
            .unwrap_or(Severity::Unknown),
    }
}

/// npm 7+: vulnerable packages by name; advisories are the `via` objects, `effects` lead
/// to the direct dependencies.
fn report_v2(v: &Value) -> Findings {
    let Some(map) = v.get("vulnerabilities").and_then(Value::as_object) else {
        return Findings::default();
    };
    let direct = |name: &str| {
        map.get(name)
            .and_then(|e| e.get("isDirect"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    // The direct dependencies above a package (following `effects` upwards).
    let roots = |name: &str| -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut out = BTreeSet::new();
        let mut todo = vec![name.to_string()];
        while let Some(n) = todo.pop() {
            if !seen.insert(n.clone()) {
                continue;
            }
            for e in map
                .get(&n)
                .map(|e| strings(e, "effects"))
                .unwrap_or_default()
            {
                if direct(&e) {
                    out.insert(e.clone());
                }
                todo.push(e);
            }
        }
        out.into_iter().collect()
    };
    let mut findings = Findings::default();
    for (name, entry) in map {
        let fix_version = entry
            .get("fixAvailable")
            .filter(|f| s(f, "name").as_deref() == Some(name.as_str()))
            .and_then(|f| s(f, "version"));
        for via in entry
            .get("via")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !via.is_object() {
                continue;
            }
            let url = s(via, "url");
            let id = url
                .as_deref()
                .and_then(ghsa_of)
                .or_else(|| via.get("source").map(|x| x.to_string()))
                .unwrap_or_default();
            let mut vuln = Vulnerability::new(
                Ecosystem::Npm,
                &s(via, "name").unwrap_or_else(|| name.clone()),
                &id,
                &s(via, "title").unwrap_or_default(),
            );
            vuln.severity = severity_of(via);
            vuln.affected = s(via, "range");
            vuln.fixed = vuln
                .affected
                .as_deref()
                .and_then(fixed_from_range)
                .or(fix_version.clone());
            vuln.url = url;
            if !direct(name) {
                vuln.via = roots(name);
            }
            findings.vulnerabilities.push(vuln);
        }
    }
    findings
}

/// npm 6 advisories (pnpm, Yarn 1 and 2/3), each with its findings (installed versions
/// and the paths that lead to them).
fn advisories<'a>(list: impl Iterator<Item = &'a Value>) -> Findings {
    let mut by_id: BTreeMap<String, Value> = BTreeMap::new();
    for a in list {
        let key = a
            .get("id")
            .map(|x| x.to_string())
            .unwrap_or_else(|| s(a, "url").unwrap_or_default());
        by_id.entry(key).or_insert_with(|| a.clone());
    }
    let mut findings = Findings::default();
    for a in by_id.values() {
        let package = s(a, "module_name").unwrap_or_default();
        if package.is_empty() {
            continue;
        }
        let url = s(a, "url");
        let ghsa = s(a, "github_advisory_id").or_else(|| url.as_deref().and_then(ghsa_of));
        let cves = strings(a, "cves");
        let mut base = Vulnerability::new(
            Ecosystem::Npm,
            &package,
            &ghsa
                .clone()
                .or_else(|| a.get("id").map(|x| x.to_string()))
                .unwrap_or_default(),
            &s(a, "title").unwrap_or_default(),
        );
        base.aliases = cves;
        base.severity = severity_of(a);
        base.affected = s(a, "vulnerable_versions");
        base.fixed = s(a, "patched_versions")
            .and_then(|p| {
                let v = p.trim().trim_start_matches(['>', '=', '^', '~']).trim();
                let v: String = v.chars().take_while(|c| !c.is_whitespace()).collect();
                v.starts_with(|c: char| c.is_ascii_digit()).then_some(v)
            })
            .or_else(|| base.affected.as_deref().and_then(fixed_from_range));
        base.url = url;
        let base = base.prefer_cve();
        let found = a.get("findings").and_then(Value::as_array);
        if found.is_none_or(|f| f.is_empty()) {
            findings.vulnerabilities.push(base);
            continue;
        }
        for f in found.into_iter().flatten() {
            let mut v = base.clone();
            v.installed = s(f, "version");
            let mut via = BTreeSet::new();
            for path in strings(f, "paths") {
                let segs: Vec<&str> = path
                    .split('>')
                    .map(str::trim)
                    // pnpm prefixes the importer: `.` or `packages__icons`.
                    .filter(|p| *p != "." && !p.contains("__"))
                    .collect();
                if segs.len() > 1 {
                    via.insert(segs[0].to_string());
                }
            }
            v.via = via.into_iter().collect();
            findings.vulnerabilities.push(v);
        }
    }
    findings
}

/// Yarn 4: `{"value": "minimist", "children": {"ID", "Issue", "URL", "Severity",
/// "Vulnerable Versions", "Tree Versions", "Dependents"}}`.
fn berry_entry(v: &Value) -> Option<Vulnerability> {
    let package = s(v, "value")?;
    let c = v.get("children")?;
    let url = s(c, "URL");
    let id = url
        .as_deref()
        .and_then(ghsa_of)
        .or_else(|| {
            c.get("ID")
                .map(|x| x.to_string().trim_matches('"').to_string())
        })
        .unwrap_or_default();
    let mut vuln = Vulnerability::new(
        Ecosystem::Npm,
        &package,
        &id,
        &s(c, "Issue").unwrap_or_default(),
    );
    vuln.severity = Severity::parse(&s(c, "Severity").unwrap_or_default());
    vuln.affected = s(c, "Vulnerable Versions");
    vuln.fixed = vuln.affected.as_deref().and_then(fixed_from_range);
    vuln.installed = strings(c, "Tree Versions").into_iter().next();
    vuln.url = url;
    vuln.via = strings(c, "Dependents")
        .iter()
        .filter(|d| !d.is_empty() && !d.contains("@workspace:"))
        .filter_map(|d| {
            // `mkdirp@npm:0.5.1`, `@scope/pkg@npm:1.0.0`.
            let at = d[1..].find('@').map(|i| i + 1).unwrap_or(d.len());
            let name = &d[..at];
            (!name.is_empty() && name != package).then(|| name.to_string())
        })
        .collect();
    Some(vuln)
}
