//! RuboCop as Forge's external annotator and formatter:
//!
//! - [`check_args`]: `rubocop --format json --force-exclusion --stdin <path>` (the editor
//!   text on stdin, the project's `.rubocop.yml` applies);
//! - [`parse_report`]: the JSON report → [`RubyProblem`]s (1-based RuboCop positions →
//!   0-based lines and columns; severities `info` / `refactor` / `convention` / `warning`
//!   / `error` / `fatal` → Forge's levels; `correctable` offenses get a quick fix);
//! - [`autocorrect_args`] / [`corrected_source`]: `rubocop --stdin <path> -a` (safe) or
//!   `-A` (all) with `--stderr`, so stdout is the corrected file only.

use serde::{Deserialize, Serialize};

/// A problem RuboCop reported.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubyProblem {
    /// Project-relative path.
    pub path: String,
    /// 0-based.
    pub line: u32,
    /// 0-based, in characters.
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    /// `error`, `warning`, `weak`, `info` (Forge's levels).
    pub severity: String,
    /// RuboCop's own severity (`convention`, `refactor`, …).
    pub level: String,
    pub message: String,
    /// The cop (`Style/StringLiterals`).
    pub cop: String,
    pub correctable: bool,
    /// `rubocop` (the diagnostics source).
    pub source: String,
}

#[derive(Debug, Deserialize)]
struct Report {
    #[serde(default)]
    files: Vec<FileReport>,
}

#[derive(Debug, Deserialize)]
struct FileReport {
    path: String,
    #[serde(default)]
    offenses: Vec<Offense>,
}

#[derive(Debug, Deserialize)]
struct Offense {
    #[serde(default)]
    severity: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    cop_name: String,
    #[serde(default)]
    corrected: bool,
    #[serde(default)]
    correctable: bool,
    location: Location,
}

#[derive(Debug, Deserialize)]
struct Location {
    #[serde(default)]
    start_line: Option<u32>,
    #[serde(default)]
    start_column: Option<u32>,
    #[serde(default)]
    last_line: Option<u32>,
    #[serde(default)]
    last_column: Option<u32>,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    column: Option<u32>,
    #[serde(default)]
    length: Option<u32>,
}

/// RuboCop's severity as Forge's level.
pub fn severity(level: &str) -> &'static str {
    match level {
        "error" | "fatal" => "error",
        "warning" => "warning",
        "convention" | "refactor" => "weak",
        _ => "info",
    }
}

/// The arguments that check one file's text (on stdin).
pub fn check_args(path: &str, extra: &[String]) -> Vec<String> {
    let mut a = vec![
        "--format".to_string(),
        "json".to_string(),
        "--force-exclusion".to_string(),
    ];
    a.extend(extra.iter().cloned());
    a.push("--stdin".into());
    a.push(path.to_string());
    a
}

/// The arguments that autocorrect one file's text: `safe` (`-a`) or all (`-A`).
pub fn autocorrect_args(path: &str, safe: bool, extra: &[String]) -> Vec<String> {
    let mut a = vec![
        if safe {
            "--autocorrect"
        } else {
            "--autocorrect-all"
        }
        .to_string(),
        "--format".into(),
        "quiet".into(),
        "--stderr".into(),
        "--force-exclusion".into(),
    ];
    a.extend(extra.iter().cloned());
    a.push("--stdin".into());
    a.push(path.to_string());
    a
}

/// The corrected text of an autocorrect run: stdout, without a report RuboCop may have
/// printed before the `====================` separator (older versions without `--stderr`).
pub fn corrected_source(stdout: &str) -> String {
    match stdout.find("====================\n") {
        Some(i) => stdout[i + 21..].to_string(),
        None => stdout.to_string(),
    }
}

/// Problems of a JSON report (the JSON may follow other output); paths are made relative
/// to `root` and a report of stdin names `stdin_path`.
pub fn parse_report(
    output: &str,
    root: &str,
    stdin_path: Option<&str>,
) -> Result<Vec<RubyProblem>, String> {
    let start = output
        .find("{\"metadata\"")
        .or_else(|| output.find("{\"files\""))
        .or_else(|| output.find('{'))
        .ok_or("RuboCop printed no JSON report")?;
    let report: Report = serde_json::from_str(output[start..].trim_end())
        .map_err(|e| format!("not a RuboCop report: {e}"))?;
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let mut out = Vec::new();
    for f in &report.files {
        let mut path = f.path.replace('\\', "/");
        if let Some(rel) = path.strip_prefix(&format!("{root}/")) {
            path = rel.to_string();
        }
        if let Some(p) = stdin_path {
            if f.path == "-" || f.path.ends_with("stdin") || report.files.len() == 1 {
                path = p.to_string();
            }
        }
        for o in &f.offenses {
            if o.corrected {
                continue;
            }
            let l = &o.location;
            let line = l.start_line.or(l.line).unwrap_or(1).max(1) - 1;
            let column = l.start_column.or(l.column).unwrap_or(1).max(1) - 1;
            let end_line = l.last_line.map(|x| x.max(1) - 1).unwrap_or(line);
            let end_column = match l.last_column {
                Some(c) => c,
                None => column + l.length.unwrap_or(1).max(1),
            };
            let message = o
                .message
                .strip_prefix(&format!("{}: ", o.cop_name))
                .unwrap_or(&o.message)
                .to_string();
            out.push(RubyProblem {
                path: path.clone(),
                line,
                column,
                end_line,
                end_column,
                severity: severity(&o.severity).into(),
                level: o.severity.clone(),
                message,
                cop: o.cop_name.clone(),
                correctable: o.correctable,
                source: "rubocop".into(),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn json_report() {
        let problems = parse_report(&fixture("rubocop.json"), "/home/anna/shop", None).unwrap();
        let rows: Vec<_> = problems
            .iter()
            .map(|p| {
                (
                    p.path.as_str(),
                    p.line,
                    p.column,
                    p.end_column,
                    p.severity.as_str(),
                    p.cop.as_str(),
                    p.correctable,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "lib/shop/cart.rb",
                    0,
                    0,
                    1,
                    "weak",
                    "Style/FrozenStringLiteralComment",
                    true
                ),
                (
                    "lib/shop/cart.rb",
                    11,
                    6,
                    13,
                    "warning",
                    "Lint/UselessAssignment",
                    true
                ),
                (
                    "lib/shop/cart.rb",
                    15,
                    12,
                    20,
                    "weak",
                    "Style/StringLiterals",
                    true
                ),
                ("lib/shop/money.rb", 3, 0, 1, "error", "Lint/Syntax", false),
            ]
        );
        assert_eq!(
            problems[1].message,
            "Useless assignment to variable - `total`."
        );
        // A stdin check reports the file it was asked about.
        let one = r#"{"metadata":{},"files":[{"path":"/tmp/x.rb","offenses":[{"severity":"convention","message":"Layout/TrailingWhitespace: Trailing whitespace detected.","cop_name":"Layout/TrailingWhitespace","corrected":false,"correctable":true,"location":{"start_line":2,"start_column":5,"last_line":2,"last_column":6,"length":2,"line":2,"column":5}}]}],"summary":{}}"#;
        let p = parse_report(&format!("warning: x\n{one}"), "/p", Some("lib/a.rb")).unwrap();
        assert_eq!(
            (p[0].path.as_str(), p[0].line, p[0].message.as_str()),
            ("lib/a.rb", 1, "Trailing whitespace detected.")
        );
        assert!(parse_report("no json", "/p", None).is_err());
    }

    #[test]
    fn arguments_and_corrections() {
        assert_eq!(
            check_args("lib/a.rb", &[]).join(" "),
            "--format json --force-exclusion --stdin lib/a.rb"
        );
        assert_eq!(
            autocorrect_args("lib/a.rb", false, &[])[0],
            "--autocorrect-all"
        );
        assert_eq!(
            corrected_source("Inspecting 1 file\nC\n====================\nputs 'x'\n"),
            "puts 'x'\n"
        );
        assert_eq!(corrected_source("puts 'x'\n"), "puts 'x'\n");
    }
}
