//! External annotators: the reports of `go vet -json` and `golangci-lint` (JSON output)
//! turned into problems of project files.
//!
//! - `go vet -json ./...` prints, on stderr, a `# package` comment and one JSON object per
//!   package: `{ "<import path>": { "<analyzer>": [ { "posn": "/abs/file.go:12:2",
//!   "message": "…" } ] } }` (an analyzer may report `{ "error": "…" }` instead).
//! - `golangci-lint run --output.json.path=stdout` (v2; `--out-format=json` in v1) prints
//!   `{ "Issues": [ { "FromLinter", "Text", "Severity", "Pos": { "Filename", "Line",
//!   "Column" }, "Replacement" } ], "Report": … }` followed by a text summary.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoProblem {
    /// Project-relative path (`/` separated).
    pub path: String,
    /// 0-based line and column (columns count bytes as Go reports them).
    pub line: u32,
    pub column: u32,
    /// `error`, `warning` or `info`.
    pub severity: String,
    pub message: String,
    /// `go vet` or `golangci-lint`.
    pub source: String,
    /// The analyzer or linter (`printf`, `errcheck`).
    pub code: String,
}

/// `path` relative to `root` (both may use `\`); paths outside stay as they are.
pub fn relative(path: &str, root: &str) -> String {
    let p = path.replace('\\', "/");
    let r = root.replace('\\', "/");
    let r = r.trim_end_matches('/');
    match p.strip_prefix(r) {
        Some(rest) if !r.is_empty() && rest.starts_with('/') => rest[1..].to_string(),
        _ => p.trim_start_matches("./").to_string(),
    }
}

/// `/abs/file.go:12:2` → (`/abs/file.go`, 11, 1). Windows drive letters keep their colon.
fn position(posn: &str) -> Option<(String, u32, u32)> {
    let (rest, last) = posn.rsplit_once(':')?;
    let last: u32 = last.parse().ok()?;
    match rest.rsplit_once(':') {
        Some((f, l)) if !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()) => Some((
            f.to_string(),
            l.parse::<u32>().ok()?.saturating_sub(1),
            last.saturating_sub(1),
        )),
        _ => Some((rest.to_string(), last.saturating_sub(1), 0)),
    }
}

/// The problems of `go vet -json` output.
pub fn parse_vet(output: &str, root: &str) -> Vec<GoProblem> {
    // Drop the `# package` comment lines, then read the JSON objects one after another.
    let json: String = output
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = Vec::new();
    for value in serde_json::Deserializer::from_str(&json)
        .into_iter::<Value>()
        .map_while(Result::ok)
    {
        let Some(packages) = value.as_object() else {
            continue;
        };
        for analyzers in packages.values() {
            let Some(analyzers) = analyzers.as_object() else {
                continue;
            };
            for (analyzer, diags) in analyzers {
                for d in diags.as_array().into_iter().flatten() {
                    let Some(posn) = d.get("posn").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some((file, line, column)) = position(posn) else {
                        continue;
                    };
                    out.push(GoProblem {
                        path: relative(&file, root),
                        line,
                        column,
                        severity: "warning".into(),
                        message: d
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        source: "go vet".into(),
                        code: analyzer.clone(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| (&a.path, a.line, a.column).cmp(&(&b.path, b.line, b.column)));
    out
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LintReport {
    #[serde(default)]
    issues: Option<Vec<LintIssue>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LintIssue {
    from_linter: String,
    text: String,
    #[serde(default)]
    severity: String,
    pos: LintPos,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LintPos {
    filename: String,
    line: u32,
    #[serde(default)]
    column: u32,
}

/// golangci-lint's severities (configurable per linter) as Forge's.
fn severity(s: &str) -> &'static str {
    match s.to_ascii_lowercase().as_str() {
        "error" | "critical" | "high" | "blocker" => "error",
        "info" | "low" | "minor" | "hint" => "info",
        _ => "warning",
    }
}

/// The problems of golangci-lint's JSON output (the first JSON document on stdout;
/// paths are relative to its working directory `cwd`, itself relative to the project).
pub fn parse_golangci(output: &str, root: &str, cwd: &str) -> Result<Vec<GoProblem>, String> {
    let start = output
        .find('{')
        .ok_or_else(|| "golangci-lint printed no JSON report".to_string())?;
    let report: LintReport = serde_json::Deserializer::from_str(&output[start..])
        .into_iter::<LintReport>()
        .next()
        .ok_or_else(|| "golangci-lint printed no JSON report".to_string())?
        .map_err(|e| format!("cannot read golangci-lint's report: {e}"))?;
    let cwd = cwd.trim_matches('/');
    let mut out: Vec<GoProblem> = report
        .issues
        .unwrap_or_default()
        .into_iter()
        .map(|i| {
            let file = i.pos.filename.replace('\\', "/");
            let path = if file.starts_with('/') || file.get(1..2) == Some(":") {
                relative(&file, root)
            } else if cwd.is_empty() {
                file.trim_start_matches("./").to_string()
            } else {
                format!("{cwd}/{}", file.trim_start_matches("./"))
            };
            GoProblem {
                path,
                line: i.pos.line.saturating_sub(1),
                column: i.pos.column.saturating_sub(1),
                severity: severity(&i.severity).into(),
                message: i.text,
                source: "golangci-lint".into(),
                code: i.from_linter,
            }
        })
        .collect();
    out.sort_by(|a, b| (&a.path, a.line, a.column).cmp(&(&b.path, b.line, b.column)));
    Ok(out)
}

/// golangci-lint's major version from `golangci-lint --version`
/// (`golangci-lint has version 2.5.0 built with …`).
pub fn golangci_major(version_output: &str) -> Option<u32> {
    let v = version_output
        .split_whitespace()
        .skip_while(|w| *w != "version")
        .nth(1)?;
    v.trim_start_matches('v').split('.').next()?.parse().ok()
}

/// The arguments of a JSON golangci-lint run for its major version.
pub fn golangci_args(major: u32, target: &str) -> Vec<String> {
    let mut args = vec!["run".to_string()];
    if major >= 2 {
        args.push("--output.json.path=stdout".into());
        args.push("--show-stats=false".into());
    } else {
        args.push("--out-format=json".into());
    }
    args.push(target.to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    const VET: &str = include_str!("../tests/fixtures/vet.json.txt");
    const LINT: &str = include_str!("../tests/fixtures/golangci-lint.json");

    #[test]
    fn reads_go_vet_json() {
        let p = parse_vet(VET, "/work/ledger");
        assert_eq!(
            p,
            [GoProblem {
                path: "internal/money/money.go".into(),
                line: 37,
                column: 1,
                severity: "warning".into(),
                message: "fmt.Printf call needs 1 arg but has 2 args".into(),
                source: "go vet".into(),
                code: "printf".into(),
            }]
        );
        assert!(parse_vet("# x\n{}\n", "/w").is_empty());
        assert_eq!(
            position("C:\\w\\a.go:3:4"),
            Some(("C:\\w\\a.go".into(), 2, 3))
        );
    }

    #[test]
    fn reads_golangci_lint_json() {
        let p = parse_golangci(LINT, "/work/ledger", "").unwrap();
        let rows: Vec<_> = p
            .iter()
            .map(|p| (p.path.as_str(), p.line, p.column, p.code.as_str(), p.severity.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("internal/money/money.go", 36, 8, "errcheck", "warning"),
                ("internal/money/money.go", 37, 1, "govet", "warning"),
            ]
        );
        assert!(p[0].message.contains("f.Write"));
        let in_sub = parse_golangci(LINT, "/work", "ledger").unwrap();
        assert_eq!(in_sub[0].path, "ledger/internal/money/money.go");
        assert!(parse_golangci("level=error msg=boom", "/w", "").is_err());
        let none = parse_golangci(r#"{"Issues":null,"Report":{}}"#, "/w", "").unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn versions_and_arguments() {
        assert_eq!(
            golangci_major("golangci-lint has version 2.5.0 built with go1.25.1"),
            Some(2)
        );
        assert_eq!(golangci_major("golangci-lint has version v1.61.0"), Some(1));
        assert_eq!(golangci_args(1, "./...")[1], "--out-format=json");
        assert_eq!(golangci_args(2, "./...")[1], "--output.json.path=stdout");
        assert_eq!(relative("/w/a/b.go", "/w/"), "a/b.go");
        assert_eq!(relative("./a.go", "/w"), "a.go");
    }
}
