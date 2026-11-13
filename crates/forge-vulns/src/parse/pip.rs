//! `pip-audit -f json`: the installed distributions of the interpreter with their
//! vulnerabilities (`{ "dependencies": [...], "fixes": [...] }`; pip-audit before 2.5
//! wrote the list alone). PyPI advisories carry no severity.

use serde_json::Value;

use crate::model::{fixed_from_patched, Ecosystem, Findings, Vulnerability};

use super::{document, first_sentence, s, strings};

pub fn parse(out: &str) -> Result<Findings, String> {
    let v = document(out, "pip-audit")?;
    let deps = match &v {
        Value::Array(a) => a.as_slice(),
        _ => v
            .get("dependencies")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default(),
    };
    let mut findings = Findings::default();
    for d in deps {
        let Some(name) = s(d, "name") else { continue };
        let version = s(d, "version");
        for x in d
            .get("vulns")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = s(x, "id").unwrap_or_default();
            let description = s(x, "description").unwrap_or_default();
            let title = s(x, "summary").unwrap_or_else(|| first_sentence(&description));
            let mut vuln = Vulnerability::new(Ecosystem::Pypi, &name, &id, &title);
            vuln.installed = version.clone();
            vuln.aliases = strings(x, "aliases");
            vuln.fixed = fixed_from_patched(&strings(x, "fix_versions"), version.as_deref());
            vuln.url = Some(format!("https://osv.dev/vulnerability/{id}"));
            findings.vulnerabilities.push(vuln.prefer_cve());
        }
    }
    Ok(findings)
}
