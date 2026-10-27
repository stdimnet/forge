//! `govulncheck -json`: a stream of messages (`config`, `progress`, `osv` entries and
//! `finding`s). A finding's first trace frame names the vulnerable module; one with a
//! function means the project's code calls the vulnerable symbol. Go advisories carry no
//! severity.

use std::collections::HashMap;

use serde_json::Value;

use crate::model::{Ecosystem, Findings, Vulnerability};

use super::{first_sentence, s, strings};

/// The affected ranges of `module` in an OSV entry (`>=0 <0.23.0`).
fn affected(osv: &Value, module: &str) -> Option<String> {
    let mut out = Vec::new();
    for a in osv
        .get("affected")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if a.pointer("/package/name").and_then(Value::as_str) != Some(module) {
            continue;
        }
        for r in a
            .get("ranges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut lo = None;
            for e in r
                .get("events")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(i) = s(e, "introduced") {
                    lo = Some(i);
                } else if let Some(f) = s(e, "fixed") {
                    out.push(match lo.take().filter(|l| l != "0") {
                        Some(l) => format!(">={l} <{f}"),
                        None => format!("<{f}"),
                    });
                }
            }
            if let Some(l) = lo {
                out.push(format!(">={l}"));
            }
        }
    }
    (!out.is_empty()).then(|| out.join(" || "))
}

pub fn parse(out: &str) -> Result<Findings, String> {
    let start = out.find('{').ok_or("govulncheck printed no JSON report")?;
    let mut osvs: HashMap<String, Value> = HashMap::new();
    // (osv, module) → (version, fixed, reachable), in report order.
    let mut found: Vec<(String, String, Option<String>, Option<String>, bool)> = Vec::new();
    for msg in serde_json::Deserializer::from_str(&out[start..]).into_iter::<Value>() {
        let msg = msg.map_err(|e| format!("cannot read the govulncheck report: {e}"))?;
        if let Some(osv) = msg.get("osv") {
            if let Some(id) = s(osv, "id") {
                osvs.insert(id, osv.clone());
            }
        }
        let Some(f) = msg.get("finding") else {
            continue;
        };
        let Some(id) = s(f, "osv") else { continue };
        let Some(frame) = f
            .get("trace")
            .and_then(Value::as_array)
            .and_then(|t| t.first())
        else {
            continue;
        };
        let Some(module) = s(frame, "module") else {
            continue;
        };
        let reachable = frame.get("function").is_some();
        let version = s(frame, "version");
        let fixed = s(f, "fixed_version");
        match found.iter_mut().find(|x| x.0 == id && x.1 == module) {
            Some(x) => x.4 |= reachable,
            None => found.push((id, module, version, fixed, reachable)),
        }
    }
    let mut findings = Findings::default();
    for (id, module, version, fixed, reachable) in found {
        let osv = osvs.get(&id);
        let title = osv
            .and_then(|o| s(o, "summary"))
            .or_else(|| {
                osv.and_then(|o| s(o, "details"))
                    .map(|d| first_sentence(&d))
            })
            .unwrap_or_else(|| id.clone());
        let mut vuln = Vulnerability::new(Ecosystem::Go, &module, &id, &title);
        vuln.installed = version;
        vuln.fixed = fixed;
        vuln.aliases = osv.map(|o| strings(o, "aliases")).unwrap_or_default();
        vuln.affected = osv.and_then(|o| affected(o, &module));
        vuln.url = Some(
            osv.and_then(|o| o.pointer("/database_specific/url"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("https://pkg.go.dev/vuln/{id}")),
        );
        vuln.reachable = reachable;
        findings.vulnerabilities.push(vuln.prefer_cve());
    }
    Ok(findings)
}
