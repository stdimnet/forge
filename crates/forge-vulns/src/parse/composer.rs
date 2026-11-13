//! `composer audit --format=json`, read by `phpforge_composer::audit` and adapted to the
//! shared model (abandoned packages become notices).

use phpforge_composer::audit::{self, AuditReport};

use crate::model::{
    fixed_from_range, Ecosystem, Findings, Notice, NoticeKind, Severity, Vulnerability,
};

pub fn parse(out: &str) -> Result<Findings, String> {
    audit::parse(out).map(|r| from_report(&r))
}

pub fn from_report(r: &AuditReport) -> Findings {
    let vulnerabilities = r
        .advisories
        .iter()
        .map(|a| {
            let mut v = Vulnerability::new(
                Ecosystem::Composer,
                &a.package_name,
                a.cve.as_deref().unwrap_or(&a.advisory_id),
                &a.title,
            );
            if a.cve.is_some() {
                v.aliases.push(a.advisory_id.clone());
            }
            v.aliases.extend(a.sources.iter().cloned());
            v.severity = Severity::parse(a.severity.as_deref().unwrap_or(""));
            v.affected = (!a.affected_versions.is_empty()).then(|| a.affected_versions.clone());
            v.fixed = fixed_from_range(&a.affected_versions);
            v.url = a.link.clone();
            v.prefer_cve()
        })
        .collect();
    let notices = r
        .abandoned
        .iter()
        .map(|a| Notice {
            ecosystem: Ecosystem::Composer,
            package: a.package_name.clone(),
            kind: NoticeKind::Abandoned,
            message: audit::abandoned_message(a),
            replacement: a.replacement.clone().filter(|r| !r.is_empty()),
            url: None,
        })
        .collect();
    Findings {
        vulnerabilities,
        notices,
    }
}
