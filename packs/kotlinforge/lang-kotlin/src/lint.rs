//! ktlint and detekt as external tools: their command lines and their reports (ktlint's
//! JSON and Checkstyle reporters, detekt's SARIF and Checkstyle XML reports) as problems
//! for the Problems view and the editor.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Which tool found a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Linter {
    Ktlint,
    Detekt,
}

impl Linter {
    pub fn label(self) -> &'static str {
        match self {
            Linter::Ktlint => "ktlint",
            Linter::Detekt => "detekt",
        }
    }
}

/// Severity as the editor shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    Weak,
    Info,
}

/// One problem of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LintProblem {
    pub linter: Linter,
    /// The file as the tool reported it (absolute, relative or `<stdin>`).
    pub file: String,
    /// 0-based line and column.
    pub line: u32,
    pub column: u32,
    pub severity: Severity,
    pub message: String,
    /// `standard:no-wildcard-imports`, `MagicNumber`.
    pub rule: String,
}

fn zero_based(n: Option<u64>) -> u32 {
    n.unwrap_or(1).max(1) as u32 - 1
}

/// ktlint `--reporter=json`: `[{ file, errors: [{ line, column, message, rule }] }]`.
pub fn parse_ktlint_json(text: &str) -> Result<Vec<LintProblem>, String> {
    // Log lines may come first: the report starts at a line that starts with `[`.
    let start = text
        .match_indices('[')
        .map(|(i, _)| i)
        .find(|&i| i == 0 || text[..i].ends_with('\n'))
        .ok_or("ktlint printed no JSON report")?;
    let v: Value =
        serde_json::from_str(&text[start..]).map_err(|e| format!("ktlint report: {e}"))?;
    let mut out = Vec::new();
    for f in v.as_array().into_iter().flatten() {
        let file = f["file"].as_str().unwrap_or_default().to_string();
        for e in f["errors"].as_array().into_iter().flatten() {
            out.push(LintProblem {
                linter: Linter::Ktlint,
                file: file.clone(),
                line: zero_based(e["line"].as_u64()),
                column: zero_based(e["column"].as_u64()),
                severity: Severity::Warning,
                message: e["message"].as_str().unwrap_or_default().to_string(),
                rule: e["rule"].as_str().unwrap_or_default().to_string(),
            });
        }
    }
    Ok(out)
}

/// detekt `--report sarif:…`: SARIF 2.1 results with their rule ids and levels.
pub fn parse_sarif(text: &str) -> Result<Vec<LintProblem>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("SARIF report: {e}"))?;
    let mut out = Vec::new();
    for run in v["runs"].as_array().into_iter().flatten() {
        for r in run["results"].as_array().into_iter().flatten() {
            let loc = &r["locations"][0]["physicalLocation"];
            let uri = loc["artifactLocation"]["uri"].as_str().unwrap_or_default();
            let file = uri.strip_prefix("file://").unwrap_or(uri).to_string();
            let rule = r["ruleId"].as_str().unwrap_or_default();
            out.push(LintProblem {
                linter: Linter::Detekt,
                file: percent_decode(&file),
                line: zero_based(loc["region"]["startLine"].as_u64()),
                column: zero_based(loc["region"]["startColumn"].as_u64()),
                severity: match r["level"].as_str().unwrap_or("warning") {
                    "error" => Severity::Error,
                    "note" => Severity::Weak,
                    "none" => Severity::Info,
                    _ => Severity::Warning,
                },
                message: r["message"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                // `detekt.style.MagicNumber` → `MagicNumber`
                rule: rule.rsplit('.').next().unwrap_or(rule).to_string(),
            });
        }
    }
    Ok(out)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#10;", "\n")
        .replace("&amp;", "&")
}

/// The attributes of the XML tag that starts at `tag` (`<error line="3" …/>`).
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq]
            .trim()
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or("")
            .to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let Some(end) = after[1..].find(q) else { break };
        out.push((name, unescape_xml(&after[1..1 + end])));
        rest = &after[end + 2..];
    }
    out
}

/// A Checkstyle XML report (ktlint `--reporter=checkstyle`, detekt `--report xml:…`).
pub fn parse_checkstyle(text: &str, linter: Linter) -> Result<Vec<LintProblem>, String> {
    if !text.contains("<checkstyle") {
        return Err("not a Checkstyle report".into());
    }
    let mut out = Vec::new();
    let mut file = String::new();
    for part in text.split('<').skip(1) {
        let tag = part.split('>').next().unwrap_or_default();
        let attr = |list: &[(String, String)], k: &str| {
            list.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
        };
        if let Some(rest) = tag.strip_prefix("file ") {
            file = attr(&attributes(rest), "name").unwrap_or_default();
        } else if let Some(rest) = tag.strip_prefix("error ") {
            let a = attributes(rest);
            let source = attr(&a, "source").unwrap_or_default();
            let rule = source
                .strip_prefix("detekt.")
                .unwrap_or(&source)
                .to_string();
            out.push(LintProblem {
                linter,
                file: file.clone(),
                line: zero_based(attr(&a, "line").and_then(|l| l.parse().ok())),
                column: zero_based(attr(&a, "column").and_then(|l| l.parse().ok())),
                severity: match attr(&a, "severity").as_deref() {
                    Some("error") => Severity::Error,
                    Some("info") => Severity::Weak,
                    _ => Severity::Warning,
                },
                message: attr(&a, "message").unwrap_or_default(),
                rule,
            });
        }
    }
    Ok(out)
}

/// Whether a reported file is `path` (absolute or project-relative, `/` or `\`).
pub fn same_file(reported: &str, path: &str) -> bool {
    let r = reported.replace('\\', "/");
    let p = path.replace('\\', "/");
    r == p
        || r.ends_with(&format!("/{}", p.trim_start_matches('/')))
        || p.ends_with(&format!("/{}", r.trim_start_matches("./")))
}

/// ktlint arguments that check (or with `format`, fix) the text on stdin as `path`.
pub fn ktlint_args(path: &str, format: bool) -> Vec<String> {
    let mut a = vec!["--stdin".to_string(), format!("--stdin-path={path}")];
    if format {
        a.push("--format".into());
    } else {
        a.push("--reporter=json".into());
    }
    a.push("--relative".into());
    a
}

/// detekt arguments that check `input` (files or folders, comma separated) with the
/// project's configuration and write a SARIF report.
pub fn detekt_args(
    input: &[String],
    config: Option<&str>,
    baseline: Option<&str>,
    report: &str,
) -> Vec<String> {
    let mut a = vec![
        "--input".to_string(),
        input.join(","),
        "--report".into(),
        format!("sarif:{report}"),
    ];
    if let Some(c) = config.filter(|c| !c.is_empty()) {
        a.push("--config".into());
        a.push(c.to_string());
        a.push("--build-upon-default-config".into());
    }
    if let Some(b) = baseline.filter(|b| !b.is_empty()) {
        a.push("--baseline".into());
        a.push(b.to_string());
    }
    a
}

/// Where a project keeps detekt's configuration and baseline (the Gradle plugin's
/// defaults), among the project `files` (relative paths).
pub fn detekt_config_files(files: &[String]) -> (Option<String>, Option<String>) {
    let find = |names: &[&str]| {
        names
            .iter()
            .find(|n| files.iter().any(|f| f == *n))
            .map(|n| n.to_string())
    };
    (
        find(&[
            "config/detekt/detekt.yml",
            "config/detekt.yml",
            "detekt.yml",
            ".detekt.yml",
        ]),
        find(&[
            "config/detekt/baseline.xml",
            "detekt-baseline.xml",
            "baseline.xml",
        ]),
    )
}

/// Whether a build file applies a linter's Gradle plugin (`org.jlleitschuh.gradle.ktlint`,
/// `com.diffplug.spotless` with ktlint, `io.gitlab.arturbosch.detekt`).
pub fn gradle_plugin(build: &str, linter: Linter) -> bool {
    match linter {
        Linter::Ktlint => {
            build.contains("org.jlleitschuh.gradle.ktlint")
                || (build.contains("spotless") && build.contains("ktlint"))
        }
        Linter::Detekt => {
            build.contains("io.gitlab.arturbosch.detekt") || build.contains("dev.detekt")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KTLINT_JSON: &str = r#"[
  {
    "file": "src/main/kotlin/com/acme/App.kt",
    "errors": [
      { "line": 3, "column": 1, "message": "Wildcard import", "rule": "standard:no-wildcard-imports" },
      { "line": 12, "column": 40, "message": "Exceeded max line length (120)", "rule": "standard:max-line-length" }
    ]
  }
]"#;

    const DETEKT_SARIF: &str = r#"{
  "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
  "version": "2.1.0",
  "runs": [{
    "tool": { "driver": { "name": "detekt", "version": "1.23.8" } },
    "results": [
      { "ruleId": "detekt.style.MagicNumber", "level": "warning",
        "message": { "text": "This expression contains a magic number. Consider defining it to a well named constant." },
        "locations": [{ "physicalLocation": {
          "artifactLocation": { "uri": "file:///home/dev/shop%20app/src/main/kotlin/com/acme/Cart.kt" },
          "region": { "startLine": 14, "startColumn": 23, "endLine": 14, "endColumn": 26 } } }] },
      { "ruleId": "detekt.complexity.LongMethod", "level": "error",
        "message": { "text": "The function checkout is too long (72)." },
        "locations": [{ "physicalLocation": {
          "artifactLocation": { "uri": "src/main/kotlin/com/acme/Checkout.kt" },
          "region": { "startLine": 5 } } }] }
    ]
  }]
}"#;

    const CHECKSTYLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<checkstyle version="4.3">
<file name="src/main/kotlin/com/acme/Cart.kt">
	<error line="14" column="23" severity="warning" message="This expression contains a &quot;magic&quot; number." source="detekt.MagicNumber" />
</file>
<file name="src/main/kotlin/com/acme/App.kt">
	<error line="3" column="1" severity="error" message="Wildcard import" source="standard:no-wildcard-imports" />
</file>
</checkstyle>"#;

    #[test]
    fn ktlint_json_report() {
        let p = parse_ktlint_json(KTLINT_JSON).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].line, p[0].column), (2, 0));
        assert_eq!(p[0].rule, "standard:no-wildcard-imports");
        assert_eq!(p[1].file, "src/main/kotlin/com/acme/App.kt");
        assert_eq!(p[1].severity, Severity::Warning);
        // Log lines before the report are skipped.
        assert_eq!(
            parse_ktlint_json(&format!("12:00 [main] INFO x\n{KTLINT_JSON}"))
                .unwrap()
                .len(),
            2
        );
        assert!(parse_ktlint_json("Exception in thread").is_err());
        assert!(parse_ktlint_json("[]").unwrap().is_empty());
    }

    #[test]
    fn detekt_sarif_report() {
        let p = parse_sarif(DETEKT_SARIF).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(
            p[0].file,
            "/home/dev/shop app/src/main/kotlin/com/acme/Cart.kt"
        );
        assert_eq!((p[0].line, p[0].column), (13, 22));
        assert_eq!(p[0].rule, "MagicNumber");
        assert_eq!(p[1].severity, Severity::Error);
        assert_eq!((p[1].line, p[1].column), (4, 0));
        assert!(same_file(&p[0].file, "src/main/kotlin/com/acme/Cart.kt"));
        assert!(!same_file(&p[0].file, "src/main/kotlin/com/acme/App.kt"));
    }

    #[test]
    fn checkstyle_reports() {
        let p = parse_checkstyle(CHECKSTYLE, Linter::Detekt).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].message, "This expression contains a \"magic\" number.");
        assert_eq!(p[0].rule, "MagicNumber");
        assert_eq!(p[1].file, "src/main/kotlin/com/acme/App.kt");
        assert_eq!(p[1].severity, Severity::Error);
        assert!(parse_checkstyle("{}", Linter::Ktlint).is_err());
    }

    #[test]
    fn command_lines_and_project_files() {
        assert_eq!(
            ktlint_args("src/App.kt", false),
            [
                "--stdin",
                "--stdin-path=src/App.kt",
                "--reporter=json",
                "--relative"
            ]
        );
        assert!(ktlint_args("a.kt", true).contains(&"--format".to_string()));
        let files = vec![
            "config/detekt/detekt.yml".to_string(),
            "build.gradle.kts".into(),
        ];
        let (config, baseline) = detekt_config_files(&files);
        assert_eq!(config.as_deref(), Some("config/detekt/detekt.yml"));
        assert!(baseline.is_none());
        let a = detekt_args(
            &["src/main/kotlin".into()],
            config.as_deref(),
            None,
            "/tmp/d.sarif",
        );
        assert_eq!(
            a,
            [
                "--input",
                "src/main/kotlin",
                "--report",
                "sarif:/tmp/d.sarif",
                "--config",
                "config/detekt/detekt.yml",
                "--build-upon-default-config"
            ]
        );
        assert!(gradle_plugin(
            "plugins { id(\"io.gitlab.arturbosch.detekt\") version \"1.23.8\" }",
            Linter::Detekt
        ));
        assert!(gradle_plugin(
            "id(\"org.jlleitschuh.gradle.ktlint\")",
            Linter::Ktlint
        ));
        assert!(!gradle_plugin("kotlin(\"jvm\")", Linter::Ktlint));
    }
}
