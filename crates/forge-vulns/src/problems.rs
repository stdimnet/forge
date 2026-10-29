//! The "Vulnerable Dependencies" inspection: a problem at each declaration of a vulnerable
//! package (or of the direct dependency that pulls a vulnerable one in), rated by its
//! worst advisory, and the "Upgrade to <fixed version>" quick fix that edits the
//! constraint.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::declare::{self, Declaration, FileKind};
use crate::model::{max_version, Notice, NoticeKind, Severity, Vulnerability};
use crate::text::Lines;

/// The quick fix of a problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Upgrade {
    pub package: String,
    /// The version that fixes all of the package's advisories.
    pub fixed: String,
    /// The constraint the fix writes (`^2.4.5`).
    pub constraint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProblem {
    pub line: u32,
    pub start: u32,
    pub end: u32,
    /// Editor severity: `error`, `warning` or `weak`.
    pub severity: String,
    /// The worst advisory's rating.
    pub rating: Severity,
    pub message: String,
    /// The package the problem is about (the transitive one for a `via` problem).
    pub package: String,
    /// The declared dependency it is reported on (`package` itself, or the direct
    /// dependency that pulls a transitive package in).
    pub dependency: String,
    /// Advisory ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<Upgrade>,
}

fn describe(v: &Vulnerability) -> String {
    format!("{} ({}, {})", v.title, v.id, v.severity.label())
}

fn named(v: &Vulnerability) -> String {
    match &v.installed {
        Some(i) => format!("{} {i}", v.package),
        None => v.package.clone(),
    }
}

fn worst(list: &[&Vulnerability]) -> Severity {
    list.iter()
        .map(|v| v.severity)
        .max_by_key(|s| s.rank())
        .unwrap_or_default()
}

fn message(package: &str, list: &[&Vulnerability], via: Option<&str>) -> String {
    let who = match via {
        Some(d) => format!("{} (a dependency of {d})", named(list[0])),
        None => list
            .first()
            .map(|v| named(v))
            .unwrap_or_else(|| package.to_string()),
    };
    let mut out = if list.len() == 1 {
        format!("{who} is vulnerable: {}", describe(list[0]))
    } else {
        let items: Vec<String> = list.iter().map(|v| describe(v)).collect();
        format!(
            "{who} has {} vulnerabilities: {}",
            list.len(),
            items.join("; ")
        )
    };
    match max_version(list.iter().filter_map(|v| v.fixed.as_deref())) {
        Some(f) if list.iter().all(|v| v.fixed.is_some()) => {
            out.push_str(&format!(". Fixed in {f}"))
        }
        Some(f) => out.push_str(&format!(". Partly fixed in {f}")),
        None => out.push_str(". No fixed version yet"),
    }
    if list.iter().any(|v| v.reachable) {
        out.push_str(". The project calls the vulnerable code");
    }
    out
}

fn problem(d: &Declaration, severity: Severity, message: String, package: &str) -> FileProblem {
    FileProblem {
        line: d.name.line,
        start: d.name.start,
        end: d.name.end,
        severity: severity.editor().to_string(),
        rating: severity,
        message,
        package: package.to_string(),
        dependency: d.package.clone(),
        ids: Vec::new(),
        upgrade: None,
    }
}

/// The problems of `path` (any manifest or lock file [`FileKind`] knows) for the
/// findings of its ecosystem.
pub fn problems(
    path: &str,
    text: &str,
    vulnerabilities: &[Vulnerability],
    notices: &[Notice],
) -> Vec<FileProblem> {
    let Some(kind) = FileKind::of(path) else {
        return Vec::new();
    };
    let eco = kind.ecosystem();
    let key = |n: &str| eco.key(n);
    let decls = declare::declarations(path, text);
    let declared: std::collections::HashSet<String> =
        decls.iter().map(|d| key(&d.package)).collect();
    let mine: Vec<&Vulnerability> = vulnerabilities
        .iter()
        .filter(|v| v.ecosystem == eco)
        .collect();
    let mut out = Vec::new();
    for d in &decls {
        let k = key(&d.package);
        let direct: Vec<&Vulnerability> = mine
            .iter()
            .copied()
            .filter(|v| key(&v.package) == k)
            .collect();
        if !direct.is_empty() {
            let sev = worst(&direct);
            let mut p = problem(d, sev, message(&d.package, &direct, None), &d.package);
            p.ids = direct.iter().map(|v| v.id.clone()).collect();
            // Every advisory needs a fix for the upgrade to help.
            let fixed = direct
                .iter()
                .all(|v| v.fixed.is_some())
                .then(|| max_version(direct.iter().filter_map(|v| v.fixed.as_deref())))
                .flatten();
            if let (Some(fixed), Some(_)) = (fixed, d.value) {
                if let Some(constraint) =
                    declare::upgraded_constraint(d.style, &d.constraint, &fixed)
                {
                    if !matches!(d.package.as_str(), "stdlib" | "toolchain") {
                        p.upgrade = Some(Upgrade {
                            package: d.package.clone(),
                            fixed: fixed.trim_start_matches('v').to_string(),
                            constraint,
                        });
                    }
                }
            }
            out.push(p);
        }
        // Transitive packages this dependency pulls in, by package.
        let mut via: BTreeMap<String, Vec<&Vulnerability>> = BTreeMap::new();
        for v in &mine {
            if !declared.contains(&key(&v.package)) && v.via.iter().any(|x| key(x) == k) {
                via.entry(v.package.clone()).or_default().push(v);
            }
        }
        for (package, list) in via {
            let sev = worst(&list);
            let mut p = problem(d, sev, message(&package, &list, Some(&d.package)), &package);
            p.ids = list.iter().map(|v| v.id.clone()).collect();
            out.push(p);
        }
        for n in notices
            .iter()
            .filter(|n| n.ecosystem == eco && key(&n.package) == k)
        {
            let sev = match n.kind {
                NoticeKind::Abandoned => "warning",
                NoticeKind::Unmaintained | NoticeKind::Yanked => "weak",
            };
            out.push(FileProblem {
                severity: sev.to_string(),
                rating: Severity::Low,
                ..problem(d, Severity::Low, n.message.clone(), &n.package)
            });
        }
    }
    out
}

/// `text` with the constraint of `package` raised to `fixed`; `None` when the package is
/// not declared with an editable constraint.
pub fn upgrade(path: &str, text: &str, package: &str, fixed: &str) -> Option<String> {
    let eco = FileKind::of(path)?.ecosystem();
    let decls = declare::declarations(path, text);
    let lines = Lines::new(text);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for d in decls
        .iter()
        .filter(|d| eco.key(&d.package) == eco.key(package))
    {
        let Some(value) = d.value else { continue };
        let Some(c) = declare::upgraded_constraint(d.style, &d.constraint, fixed) else {
            continue;
        };
        let (s, e) = lines.range(value)?;
        edits.push((s, e, c));
    }
    if edits.is_empty() {
        return None;
    }
    let mut out = text.to_string();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (s, e, c) in edits {
        out.replace_range(s..e, &c);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Ecosystem;

    fn vuln(
        eco: Ecosystem,
        package: &str,
        id: &str,
        sev: Severity,
        fixed: Option<&str>,
    ) -> Vulnerability {
        let mut v = Vulnerability::new(eco, package, id, "Bad thing");
        v.severity = sev;
        v.fixed = fixed.map(str::to_string);
        v
    }

    #[test]
    fn reports_direct_and_transitive_packages() {
        let pkg = "{\n  \"dependencies\": {\n    \"lodash\": \"^4.17.20\"\n  },\n  \"devDependencies\": {\n    \"vite\": \"^6.3.5\"\n  }\n}\n";
        let mut lodash1 = vuln(
            Ecosystem::Npm,
            "lodash",
            "CVE-2020-8203",
            Severity::High,
            Some("4.17.19"),
        );
        lodash1.installed = Some("4.17.15".into());
        let lodash2 = vuln(
            Ecosystem::Npm,
            "lodash",
            "CVE-2021-23337",
            Severity::High,
            Some("4.17.21"),
        );
        let mut esbuild = vuln(
            Ecosystem::Npm,
            "esbuild",
            "GHSA-67mh-4wv8-2f99",
            Severity::Moderate,
            Some("0.25.0"),
        );
        esbuild.via = vec!["vite".into()];
        let other = vuln(
            Ecosystem::Pypi,
            "lodash",
            "PYSEC-1",
            Severity::Critical,
            None,
        );
        let p = problems(
            "package.json",
            pkg,
            &[lodash1, lodash2, esbuild, other],
            &[],
        );
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].line, 2);
        assert_eq!(p[0].severity, "error");
        assert_eq!(
            p[0].message,
            "lodash 4.17.15 has 2 vulnerabilities: Bad thing (CVE-2020-8203, High); Bad thing (CVE-2021-23337, High). Fixed in 4.17.21"
        );
        assert_eq!(
            p[0].upgrade,
            Some(Upgrade {
                package: "lodash".into(),
                fixed: "4.17.21".into(),
                constraint: "^4.17.21".into()
            })
        );
        assert_eq!(p[1].line, 5);
        assert_eq!(p[1].package, "esbuild");
        assert_eq!(p[1].dependency, "vite");
        assert_eq!(p[1].severity, "warning");
        assert!(p[1]
            .message
            .starts_with("esbuild (a dependency of vite) is vulnerable"));
        assert_eq!(p[1].upgrade, None);
    }

    #[test]
    fn upgrades_constraints() {
        let req = "Django==3.2.0\nrequests>=2.0\n";
        let v = vuln(
            Ecosystem::Pypi,
            "django",
            "CVE-1",
            Severity::Unknown,
            Some("3.2.25"),
        );
        let p = problems("requirements.txt", req, &[v], &[]);
        assert_eq!(p[0].upgrade.as_ref().unwrap().constraint, "==3.2.25");
        assert_eq!(
            upgrade("requirements.txt", req, "django", "3.2.25").as_deref(),
            Some("Django==3.2.25\nrequests>=2.0\n")
        );
        let gomod = "module x\n\ngo 1.21\n\nrequire golang.org/x/net v0.17.0\n";
        assert_eq!(
            upgrade("go.mod", gomod, "golang.org/x/net", "v0.23.0").as_deref(),
            Some("module x\n\ngo 1.21\n\nrequire golang.org/x/net v0.23.0\n")
        );
        assert_eq!(upgrade("go.mod", gomod, "golang.org/x/text", "1.0.0"), None);
    }

    #[test]
    fn standard_library_findings_get_no_fix() {
        let gomod = "module x\n\ngo 1.21.0\n";
        let v = vuln(
            Ecosystem::Go,
            "stdlib",
            "GO-2024-2687",
            Severity::Unknown,
            Some("v1.21.9"),
        );
        let p = problems("go.mod", gomod, &[v], &[]);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].line, p[0].start), (2, 3));
        assert!(p[0].upgrade.is_none());
    }

    #[test]
    fn notices_and_lock_files() {
        let lock = "{\n  \"packages\": [\n    {\"name\": \"swiftmailer/swiftmailer\", \"version\": \"6.3.0\"}\n  ]\n}";
        let n = Notice {
            ecosystem: Ecosystem::Composer,
            package: "swiftmailer/swiftmailer".into(),
            kind: NoticeKind::Abandoned,
            message: "abandoned".into(),
            replacement: None,
            url: None,
        };
        let p = problems("composer.lock", lock, &[], &[n]);
        assert_eq!(p[0].severity, "warning");
        assert_eq!(p[0].line, 2);
    }
}
