//! The tools' JSON reports:
//!
//! | Tool | Output |
//! | --- | --- |
//! | ESLint | `--format json`: one result per file with `messages` (1-based), `output` after `--fix-dry-run` |
//! | Stylelint | `--formatter json`: one result per file with `warnings` (stdout before 16, stderr since) |
//! | Biome | `--reporter=json`: `diagnostics` with byte spans (1.x) or line/column positions |
//!
//! Text before or after the JSON (npm warnings, deprecation notices) is ignored.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::WebTool;
use crate::problem::{line_col, PathContext, Problem, Severity};

/// A fixed text a tool printed for one file (ESLint's `output`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOutput {
    pub file: String,
    pub text: String,
}

/// A parsed report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub problems: Vec<Problem>,
    pub outputs: Vec<FileOutput>,
    /// Errors of the tool itself (an invalid option in its configuration).
    pub errors: Vec<String>,
}

/// The first JSON object or array in `text`.
pub fn extract_json(text: &str) -> Option<Value> {
    for (i, c) in text.char_indices() {
        if c != '{' && c != '[' {
            continue;
        }
        let mut stream = serde_json::Deserializer::from_str(&text[i..]).into_iter::<Value>();
        if let Some(Ok(v)) = stream.next() {
            if v.is_object() || v.is_array() {
                return Some(v);
            }
        }
    }
    None
}

/// Whether `json` has the shape of `tool`'s report.
fn is_report(tool: WebTool, json: &Value) -> bool {
    match tool {
        WebTool::Eslint => json
            .as_array()
            .is_some_and(|a| a.iter().all(|r| r.get("messages").is_some())),
        WebTool::Stylelint => json
            .as_array()
            .is_some_and(|a| a.iter().all(|r| r.get("warnings").is_some())),
        WebTool::Biome => json.get("diagnostics").is_some(),
        WebTool::Prettier => false,
    }
}

/// The report in `stdout` (or `stderr`: Stylelint 16 prints it there). `Err` when neither
/// holds one. `source` gives a reported file's text (for Biome's byte offsets).
pub fn parse_report(
    tool: WebTool,
    stdout: &str,
    stderr: &str,
    ctx: &PathContext,
    source: &dyn Fn(&str) -> Option<String>,
) -> Result<Report, String> {
    let json = [stdout, stderr]
        .iter()
        .filter_map(|t| extract_json(t))
        .find(|j| is_report(tool, j))
        .ok_or_else(|| format!("{} printed no JSON report", tool.label()))?;
    Ok(match tool {
        WebTool::Eslint => eslint(&json, ctx),
        WebTool::Stylelint => stylelint(&json, ctx),
        WebTool::Biome => biome(&json, ctx, source),
        WebTool::Prettier => Report::default(),
    })
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn u32_of(v: &Value, key: &str) -> Option<u32> {
    v.get(key).and_then(Value::as_u64).map(|n| n as u32)
}

/// 1-based line/column to 0-based; missing or 0 counts as the start.
fn zero(n: Option<u32>) -> u32 {
    n.unwrap_or(1).saturating_sub(1)
}

fn eslint_link(rule: &str) -> Option<String> {
    if !rule.contains('/') {
        return Some(format!("https://eslint.org/docs/latest/rules/{rule}"));
    }
    rule.strip_prefix("@typescript-eslint/")
        .map(|r| format!("https://typescript-eslint.io/rules/{r}"))
}

fn eslint(json: &Value, ctx: &PathContext) -> Report {
    let mut report = Report::default();
    for result in json.as_array().into_iter().flatten() {
        let file = ctx.localize(str_of(result, "filePath").unwrap_or(""));
        if let Some(out) = str_of(result, "output") {
            report.outputs.push(FileOutput {
                file: file.clone(),
                text: out.to_string(),
            });
        }
        for m in result["messages"].as_array().into_iter().flatten() {
            let message = str_of(m, "message").unwrap_or("").trim().to_string();
            let rule = str_of(m, "ruleId").map(str::to_string);
            // "File ignored because of a matching ignore pattern": not a problem of the code.
            if rule.is_none() && message.starts_with("File ignored") {
                continue;
            }
            let severity = match m["severity"].as_u64() {
                Some(2) => Severity::Error,
                _ => Severity::Warning,
            };
            let line = zero(u32_of(m, "line"));
            let column = zero(u32_of(m, "column"));
            report.problems.push(Problem {
                tool: WebTool::Eslint,
                file: file.clone(),
                line,
                column,
                end_line: u32_of(m, "endLine").map(|l| zero(Some(l))),
                end_column: u32_of(m, "endColumn").map(|c| zero(Some(c))),
                severity,
                message,
                link: rule.as_deref().and_then(eslint_link),
                fixable: m.get("fix").is_some_and(|f| f.is_object()),
                rule,
            });
        }
    }
    report
}

/// "Unexpected invalid hex color "#ffg" (color-no-invalid-hex)" without the rule suffix.
fn strip_rule(text: &str, rule: Option<&str>) -> String {
    let text = text.trim();
    match rule {
        Some(r) => text
            .strip_suffix(&format!("({r})"))
            .map(str::trim_end)
            .unwrap_or(text)
            .to_string(),
        None => text.to_string(),
    }
}

fn stylelint(json: &Value, ctx: &PathContext) -> Report {
    let mut report = Report::default();
    for result in json.as_array().into_iter().flatten() {
        let source = str_of(result, "source").unwrap_or("");
        // Text from stdin without --stdin-filename: `<input css 1>`.
        let file = if source.starts_with("<input") {
            ctx.localize("<input>")
        } else {
            ctx.localize(source)
        };
        for w in result["invalidOptionWarnings"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(t) = str_of(w, "text") {
                report.errors.push(t.to_string());
            }
        }
        for w in result["warnings"].as_array().into_iter().flatten() {
            let rule = str_of(w, "rule").map(str::to_string);
            let syntax = rule.as_deref() == Some("CssSyntaxError");
            let severity = match str_of(w, "severity") {
                Some("warning") => Severity::Warning,
                _ => Severity::Error,
            };
            report.problems.push(Problem {
                tool: WebTool::Stylelint,
                file: file.clone(),
                line: zero(u32_of(w, "line")),
                column: zero(u32_of(w, "column")),
                end_line: u32_of(w, "endLine").map(|l| zero(Some(l))),
                end_column: u32_of(w, "endColumn").map(|c| zero(Some(c))),
                severity,
                message: strip_rule(str_of(w, "text").unwrap_or(""), rule.as_deref()),
                link: rule
                    .as_deref()
                    .filter(|r| !syntax && !r.contains('/'))
                    .map(|r| format!("https://stylelint.io/user-guide/rules/{r}")),
                // Stylelint --fix handles what its rules can fix; the others stay.
                fixable: !syntax,
                rule,
            });
        }
    }
    report
}

/// `noDoubleEquals` → `no-double-equals`.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if !out.is_empty() {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A message as text: a string, or Biome's markup (`[{ "content": "Use " }, …]`).
fn markup(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(|p| str_of(p, "content"))
                .collect::<String>(),
        ),
        _ => None,
    }
}

/// A Biome position: `{ "line": 1, "column": 5 }` (1-based).
fn position(v: &Value) -> Option<(u32, u32)> {
    v.get("line")?;
    Some((zero(u32_of(v, "line")), zero(u32_of(v, "column"))))
}

fn biome(json: &Value, ctx: &PathContext, source: &dyn Fn(&str) -> Option<String>) -> Report {
    let mut report = Report::default();
    let mut texts: std::collections::HashMap<String, Option<String>> = Default::default();
    for d in json["diagnostics"].as_array().into_iter().flatten() {
        let category = str_of(d, "category").unwrap_or("").to_string();
        let location = &d["location"];
        let reported = match &location["path"] {
            Value::String(s) => s.clone(),
            Value::Object(o) => o
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        };
        let message = str_of(d, "description")
            .map(str::to_string)
            .or_else(|| d.get("message").and_then(markup))
            .unwrap_or_default();
        let message = message.lines().next().unwrap_or("").trim().to_string();
        if reported.is_empty() && ctx.stdin_file.is_none() {
            // A diagnostic of the run itself (no files processed, invalid configuration).
            if !message.is_empty() {
                report.errors.push(message);
            }
            continue;
        }
        let file = ctx.localize(&reported);
        let range = match (
            location.get("start").and_then(position),
            location.get("end").and_then(position),
        ) {
            (Some(s), e) => Some((s, e)),
            _ => location["span"].as_array().and_then(|span| {
                let (from, to) = (span.first()?.as_u64()?, span.get(1)?.as_u64()?);
                let text = location
                    .get("sourceCode")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        texts
                            .entry(file.clone())
                            .or_insert_with(|| source(&file))
                            .clone()
                    })?;
                Some((
                    line_col(&text, from as usize),
                    Some(line_col(&text, to as usize)),
                ))
            }),
        };
        let ((line, column), end) = range.unwrap_or(((0, 0), None));
        let severity = match str_of(d, "severity") {
            Some("warning") => Severity::Warning,
            Some("information") => Severity::Weak,
            Some("hint") => Severity::Info,
            _ => Severity::Error,
        };
        let tags: Vec<&str> = d["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let link = category
            .strip_prefix("lint/")
            .and_then(|r| r.rsplit('/').next())
            .map(|name| format!("https://biomejs.dev/linter/rules/{}/", kebab(name)));
        report.problems.push(Problem {
            tool: WebTool::Biome,
            file,
            line,
            column,
            end_line: end.map(|e| e.0),
            end_column: end.map(|e| e.1),
            severity,
            message,
            fixable: tags.contains(&"fixable"),
            link,
            rule: Some(category).filter(|c| !c.is_empty()),
        });
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eslint_skips_ignored_files_and_maps_fatal_errors() {
        let out = r#"[{"filePath":"/p/src/a.js","messages":[
            {"ruleId":null,"fatal":true,"severity":2,"message":"Parsing error: Unexpected token )","line":3,"column":7},
            {"ruleId":null,"severity":1,"message":"File ignored because of a matching ignore pattern. Use \"--no-ignore\" to override."}
        ],"errorCount":1,"warningCount":1}]"#;
        let r = parse_report(
            WebTool::Eslint,
            out,
            "",
            &PathContext::new("/p", "/p"),
            &|_| None,
        )
        .unwrap();
        assert_eq!(r.problems.len(), 1);
        let p = &r.problems[0];
        assert_eq!((p.line, p.column, p.severity), (2, 6, Severity::Error));
        assert_eq!(p.rule, None);
        assert!(!p.fixable);
    }

    #[test]
    fn stylelint_16_prints_its_report_on_stderr() {
        let err = r#"[{"source":"<input css 1>","warnings":[{"line":1,"column":4,"rule":"block-no-empty","severity":"warning","text":"Unexpected empty block (block-no-empty)"}],"invalidOptionWarnings":[]}]"#;
        let ctx = PathContext::new("/p", "/p").with_stdin("a.css");
        let r = parse_report(WebTool::Stylelint, "a {}", err, &ctx, &|_| None).unwrap();
        assert_eq!(r.problems[0].file, "a.css");
        assert_eq!(r.problems[0].message, "Unexpected empty block");
        assert_eq!(r.problems[0].severity, Severity::Warning);
        assert!(parse_report(WebTool::Stylelint, "a {}", "", &ctx, &|_| None).is_err());
    }

    #[test]
    fn biome_positions_by_line_and_column() {
        let out = r#"{"diagnostics":[{"category":"lint/style/useConst","severity":"warning","message":"This let declares a variable that is only assigned once.","location":{"path":"src/a.ts","start":{"line":2,"column":1},"end":{"line":2,"column":4}},"tags":["fixable"]}]}"#;
        let r = parse_report(
            WebTool::Biome,
            out,
            "",
            &PathContext::new("/p", "/p"),
            &|_| None,
        )
        .unwrap();
        let p = &r.problems[0];
        assert_eq!(
            (p.line, p.column, p.end_line, p.end_column),
            (1, 0, Some(1), Some(3))
        );
        assert_eq!(
            p.link.as_deref(),
            Some("https://biomejs.dev/linter/rules/use-const/")
        );
        assert!(p.fixable);
        assert_eq!(kebab("noDoubleEquals"), "no-double-equals");
    }
}
