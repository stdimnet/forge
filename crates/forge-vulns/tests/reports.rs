//! Reports recorded from each audit tool, read into the shared model.

use forge_vulns::parse::parse;
use forge_vulns::{problems, Ecosystem, Findings, NoticeKind, Severity, Tool, Vulnerability};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn find<'a>(f: &'a Findings, package: &str) -> &'a Vulnerability {
    f.vulnerabilities
        .iter()
        .find(|v| v.package == package)
        .unwrap_or_else(|| panic!("no {package} in {:#?}", f.vulnerabilities))
}

#[test]
fn composer_audit() {
    let f = parse(Tool::Composer, &fixture("composer-audit.json")).unwrap();
    assert_eq!(f.vulnerabilities.len(), 2);
    let psr7 = find(&f, "guzzlehttp/psr7");
    assert_eq!(psr7.ecosystem, Ecosystem::Composer);
    assert_eq!(psr7.id, "CVE-2023-29197");
    assert!(psr7.aliases.contains(&"GHSA-wxmh-65f7-jcvw".to_string()));
    assert_eq!(psr7.severity, Severity::Moderate);
    assert_eq!(psr7.fixed.as_deref(), Some("2.4.5"));
    assert_eq!(f.notices.len(), 1);
    assert_eq!(f.notices[0].kind, NoticeKind::Abandoned);
    assert_eq!(f.notices[0].replacement.as_deref(), Some("symfony/mailer"));
}

#[test]
fn npm_audit() {
    let f = parse(Tool::Npm, &fixture("npm-audit.json")).unwrap();
    assert_eq!(f.vulnerabilities.len(), 3);
    // Worst first.
    assert_eq!(f.vulnerabilities[0].severity, Severity::Critical);
    let esbuild = find(&f, "esbuild");
    assert_eq!(esbuild.id, "GHSA-67mh-4wv8-2f99");
    assert_eq!(esbuild.via, ["vite"]);
    // `<=0.24.2` names no fixed version; npm's own fix is for vite, not esbuild.
    assert_eq!(esbuild.fixed, None);
    let lodash: Vec<&Vulnerability> = f
        .vulnerabilities
        .iter()
        .filter(|v| v.package == "lodash")
        .collect();
    assert_eq!(lodash.len(), 2);
    assert!(lodash.iter().all(|v| v.via.is_empty()));
    assert_eq!(lodash[1].fixed.as_deref(), Some("4.17.21"));
    assert_eq!(
        parse(Tool::Npm, &fixture("npm-audit-nolock.json")).unwrap_err(),
        "This command requires an existing lockfile."
    );
}

#[test]
fn pnpm_audit() {
    let f = parse(Tool::Pnpm, &fixture("pnpm-audit.json")).unwrap();
    assert_eq!(f.vulnerabilities.len(), 2);
    let router = &f.vulnerabilities[0];
    assert_eq!(router.package, "react-router");
    assert_eq!(router.id, "CVE-2025-43865");
    assert_eq!(router.aliases, ["GHSA-cpj6-fhp6-mr6j"]);
    assert_eq!(router.installed.as_deref(), Some("7.5.1"));
    assert_eq!(router.fixed.as_deref(), Some("7.5.2"));
    assert_eq!(router.via, ["react-router-dom"]);
    let esbuild = find(&f, "esbuild");
    assert_eq!(esbuild.via, ["vite", "vitest"]);
    assert_eq!(esbuild.fixed.as_deref(), Some("0.25.0"));
}

#[test]
fn yarn_audits() {
    let classic = parse(Tool::YarnClassic, &fixture("yarn-classic-audit.ndjson")).unwrap();
    let berry = parse(Tool::YarnBerry, &fixture("yarn-berry-audit.ndjson")).unwrap();
    for f in [&classic, &berry] {
        assert_eq!(f.vulnerabilities.len(), 2);
        let moment = find(f, "moment");
        assert_eq!(moment.severity, Severity::High);
        assert_eq!(moment.installed.as_deref(), Some("2.29.1"));
        assert_eq!(moment.fixed.as_deref(), Some("2.29.4"));
        assert_eq!(moment.via, ["chart.js"]);
        assert!(find(f, "axios").via.is_empty());
    }
    assert_eq!(find(&classic, "moment").id, "CVE-2022-31129");
    assert_eq!(find(&berry, "moment").id, "GHSA-wc69-rhjr-hc9g");
}

#[test]
fn pip_audit() {
    let f = parse(Tool::PipAudit, &fixture("pip-audit.json")).unwrap();
    assert_eq!(f.vulnerabilities.len(), 2);
    let django = find(&f, "django");
    assert_eq!(django.id, "CVE-2021-33203");
    assert_eq!(django.installed.as_deref(), Some("3.2.0"));
    assert_eq!(django.fixed.as_deref(), Some("3.2.4"));
    assert_eq!(django.severity, Severity::Unknown);
    assert_eq!(
        django.url.as_deref(),
        Some("https://osv.dev/vulnerability/PYSEC-2021-98")
    );
    assert!(django.title.starts_with("Django before 2.2.24"));
    assert_eq!(find(&f, "urllib3").fixed.as_deref(), Some("1.26.17"));
}

#[test]
fn cargo_audit() {
    let f = parse(Tool::CargoAudit, &fixture("cargo-audit.json")).unwrap();
    let rsa = find(&f, "rsa");
    assert_eq!(rsa.id, "CVE-2023-49092");
    assert_eq!(rsa.severity, Severity::Moderate);
    assert_eq!(rsa.fixed, None);
    let mio = find(&f, "mio");
    assert_eq!(mio.id, "RUSTSEC-2024-0019");
    assert_eq!(mio.fixed.as_deref(), Some("0.8.11"));
    assert_eq!(mio.severity, Severity::Moderate);
    let kinds: Vec<NoticeKind> = f.notices.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, [NoticeKind::Unmaintained, NoticeKind::Yanked]);
}

#[test]
fn govulncheck() {
    let f = parse(Tool::Govulncheck, &fixture("govulncheck.json")).unwrap();
    assert_eq!(f.vulnerabilities.len(), 2);
    let net = find(&f, "golang.org/x/net");
    assert_eq!(net.id, "CVE-2023-45288");
    assert_eq!(net.installed.as_deref(), Some("v0.17.0"));
    assert_eq!(net.fixed.as_deref(), Some("v0.23.0"));
    assert!(net.reachable);
    assert_eq!(net.affected.as_deref(), Some("<0.23.0"));
    assert_eq!(
        net.url.as_deref(),
        Some("https://pkg.go.dev/vuln/GO-2024-2687")
    );
    let std = find(&f, "stdlib");
    assert!(!std.reachable);
    assert_eq!(
        std.affected.as_deref(),
        Some("<1.21.9 || >=1.22.0-0 <1.22.2")
    );
}

#[test]
fn dependency_check() {
    let f = parse(
        Tool::DependencyCheck,
        &fixture("dependency-check-report.json"),
    )
    .unwrap();
    assert_eq!(f.vulnerabilities.len(), 1);
    let log4j = &f.vulnerabilities[0];
    assert_eq!(log4j.package, "org.apache.logging.log4j:log4j-core");
    assert_eq!(log4j.installed.as_deref(), Some("2.14.1"));
    assert_eq!(log4j.severity, Severity::Critical);
    assert_eq!(log4j.fixed.as_deref(), Some("2.15.0"));
    assert_eq!(log4j.affected.as_deref(), Some(">=2.13.0 <2.15.0"));
}

#[test]
fn reports_become_manifest_problems() {
    let f = parse(Tool::Pnpm, &fixture("pnpm-audit.json")).unwrap();
    let pkg = "{\n  \"dependencies\": {\n    \"react-router-dom\": \"^7.5.1\"\n  },\n  \"devDependencies\": {\n    \"vite\": \"^6.1.0\",\n    \"vitest\": \"^3.0.0\"\n  }\n}\n";
    let p = problems::problems("package.json", pkg, &f.vulnerabilities, &f.notices);
    let lines: Vec<(u32, &str)> = p.iter().map(|p| (p.line, p.package.as_str())).collect();
    assert_eq!(lines, [(2, "react-router"), (5, "esbuild"), (6, "esbuild")]);
    assert_eq!(p[0].severity, "error");

    let f = parse(Tool::CargoAudit, &fixture("cargo-audit.json")).unwrap();
    let toml =
        "[dependencies]\nrsa = \"0.9\"\nmio = { version = \"0.8.10\", features = [\"os-poll\"] }\n";
    let p = problems::problems("Cargo.toml", toml, &f.vulnerabilities, &f.notices);
    assert_eq!(p.len(), 2);
    assert!(p[0].upgrade.is_none());
    let up = p[1].upgrade.as_ref().unwrap();
    assert_eq!(
        (up.fixed.as_str(), up.constraint.as_str()),
        ("0.8.11", "0.8.11")
    );
    assert_eq!(
        problems::upgrade("Cargo.toml", toml, "mio", &up.fixed).unwrap(),
        "[dependencies]\nrsa = \"0.9\"\nmio = { version = \"0.8.11\", features = [\"os-poll\"] }\n"
    );
}
