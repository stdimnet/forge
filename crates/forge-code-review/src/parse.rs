//! The model's answers as structured data. Models wrap JSON in prose or Markdown fences,
//! rename fields and invent severities, so parsing is lenient: the first JSON value in the
//! text is used, known synonyms are mapped and anything unusable is dropped.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How serious a finding or risk is.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    #[default]
    Warning,
    Info,
}

impl Severity {
    /// Map the words models use (`critical`, `high`, `nit`, …).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "error" | "critical" | "blocker" | "high" | "major" | "bug" | "security" => Self::Error,
            "info" | "low" | "minor" | "nit" | "note" | "suggestion" | "style" | "hint" => {
                Self::Info
            }
            _ => Self::Warning,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Risk {
    pub title: String,
    pub detail: String,
    pub severity: Severity,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub path: String,
    pub summary: String,
}

/// "Summarize PR": what the change does, where it is risky and how to test it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Markdown.
    pub overview: String,
    pub risks: Vec<Risk>,
    pub files: Vec<FileSummary>,
    pub test_plan: Vec<String>,
}

/// A finding as the model wrote it, before it is matched to the diff.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFinding {
    pub path: String,
    pub line: u32,
    pub end_line: Option<u32>,
    pub severity: Severity,
    pub title: String,
    pub explanation: String,
    /// Replacement code for the line(s), without fences.
    pub suggestion: Option<String>,
}

/// At most this many findings are kept per answer.
pub const MAX_FINDINGS: usize = 40;

/// The first complete JSON object or array in `text` (inside fences or prose).
pub fn extract_json(text: &str) -> Option<Value> {
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(off) = text[start..].find(['{', '[']) {
        let begin = start + off;
        if let Some(end) = matching_end(bytes, begin) {
            if let Ok(v) = serde_json::from_str::<Value>(&text[begin..=end]) {
                return Some(v);
            }
        }
        start = begin + 1;
    }
    None
}

/// Index of the bracket closing the one at `begin`, honouring strings.
fn matching_end(b: &[u8], begin: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &c) in b.iter().enumerate().skip(begin) {
        if in_str {
            match c {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Array(a) => a.iter().map(text_of).collect::<Vec<_>>().join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The first present field of `keys` as text.
fn field(v: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| v.get(*k).filter(|x| !x.is_null()))
        .map(text_of)
        .unwrap_or_default()
}

fn list<'a>(v: &'a Value, keys: &[&str]) -> &'a [Value] {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_array))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn number(v: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|k| match v.get(*k)? {
        Value::Number(n) => n.as_u64().map(|n| n as u32),
        Value::String(s) => s.trim().split(['-', ':', ' ']).next()?.parse().ok(),
        _ => None,
    })
}

/// Parse a summary answer; prose without JSON becomes the overview.
pub fn parse_summary(text: &str) -> Summary {
    let Some(v) = extract_json(text).filter(Value::is_object) else {
        return Summary {
            overview: strip_fences(text),
            ..Summary::default()
        };
    };
    Summary {
        overview: field(&v, &["overview", "summary", "description"]),
        risks: list(&v, &["risks", "riskAreas", "risk_areas"])
            .iter()
            .filter_map(|r| {
                if let Value::String(s) = r {
                    return Some(Risk {
                        title: s.trim().into(),
                        ..Risk::default()
                    });
                }
                let title = field(r, &["title", "area", "name"]);
                let detail = field(r, &["detail", "details", "description", "explanation"]);
                (!title.is_empty() || !detail.is_empty()).then(|| Risk {
                    title,
                    detail,
                    severity: Severity::parse(&field(r, &["severity", "level", "impact"])),
                    files: list(r, &["files", "paths"]).iter().map(text_of).collect(),
                })
            })
            .collect(),
        files: list(&v, &["files", "perFile", "fileSummaries", "file_summaries"])
            .iter()
            .filter_map(|f| {
                let path = field(f, &["path", "file"]);
                let summary = field(f, &["summary", "description", "changes"]);
                (!path.is_empty() && !summary.is_empty()).then_some(FileSummary { path, summary })
            })
            .collect(),
        test_plan: list(&v, &["testPlan", "test_plan", "tests", "testing"])
            .iter()
            .map(text_of)
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

/// Parse an AI Review answer: `{"findings": [...]}` or a bare array.
pub fn parse_findings(text: &str) -> Vec<RawFinding> {
    let Some(v) = extract_json(text) else {
        return Vec::new();
    };
    let items: &[Value] = match &v {
        Value::Array(a) => a,
        other => list(other, &["findings", "comments", "issues"]),
    };
    items
        .iter()
        .filter_map(|f| {
            let path = field(f, &["path", "file"]);
            let line = number(f, &["line", "startLine", "start_line", "lineNumber"])?;
            let explanation = field(f, &["explanation", "body", "message", "description"]);
            if path.is_empty() || line == 0 || explanation.is_empty() {
                return None;
            }
            let suggestion = field(
                f,
                &["suggestion", "suggestedChange", "suggested_change", "fix"],
            );
            Some(RawFinding {
                path: path
                    .trim_start_matches("./")
                    .trim_start_matches("b/")
                    .into(),
                line,
                end_line: number(f, &["endLine", "end_line"]).filter(|e| *e > line),
                severity: Severity::parse(&field(f, &["severity", "level", "priority"])),
                title: field(f, &["title", "summary", "category"]),
                explanation,
                suggestion: Some(strip_fences(&suggestion)).filter(|s| !s.trim().is_empty()),
            })
        })
        .take(MAX_FINDINGS)
        .collect()
}

/// Remove a surrounding Markdown code fence.
pub fn strip_fences(text: &str) -> String {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let body = rest.split_once('\n').map(|(_, b)| b).unwrap_or("");
        return body
            .trim_end()
            .trim_end_matches("```")
            .trim_end()
            .to_string();
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_json_in_prose_and_fences() {
        let v = extract_json("Sure! Here it is:\n```json\n{\"a\": \"x}\", \"b\": [1]}\n```\nDone.")
            .unwrap();
        assert_eq!(v["a"], "x}");
        assert!(extract_json("no json {here").is_none());
        // A broken first object does not hide a later one.
        assert_eq!(
            extract_json("{oops} [1, 2]").unwrap(),
            serde_json::json!([1, 2])
        );
    }

    #[test]
    fn parses_summaries() {
        let s = parse_summary(
            r#"```json
{"overview":"Adds roles.","risks":[{"title":"Migration","detail":"NULL roles","severity":"high","files":["a.php"]},"Plain risk"],
 "files":[{"path":"a.php","summary":"Adds a cast"},{"path":"","summary":"x"}],
 "testPlan":["Run the migration","Log in as admin"]}
```"#,
        );
        assert_eq!(s.overview, "Adds roles.");
        assert_eq!(s.risks.len(), 2);
        assert_eq!(
            (s.risks[0].severity, s.risks[0].files.len()),
            (Severity::Error, 1)
        );
        assert_eq!(s.risks[1].title, "Plain risk");
        assert_eq!(s.files.len(), 1);
        assert_eq!(s.test_plan.len(), 2);
        let prose = parse_summary("This change adds roles.");
        assert_eq!(prose.overview, "This change adds roles.");
    }

    #[test]
    fn parses_findings_leniently() {
        let f = parse_findings(
            r#"{"findings":[
              {"path":"./app/User.php","line":"42","endLine":44,"severity":"critical","title":"Null role","explanation":"Cast fails","suggestion":"```php\n'role' => UserRole::class,\n```"},
              {"file":"b/app/B.php","line":3,"severity":"nit","message":"Typo"},
              {"path":"app/C.php","line":0,"explanation":"no line"},
              {"path":"app/D.php","line":5,"explanation":""}
            ]}"#,
        );
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].path, "app/User.php");
        assert_eq!((f[0].line, f[0].end_line), (42, Some(44)));
        assert_eq!(f[0].severity, Severity::Error);
        assert_eq!(
            f[0].suggestion.as_deref(),
            Some("'role' => UserRole::class,")
        );
        assert_eq!(
            (f[1].path.as_str(), f[1].severity),
            ("app/B.php", Severity::Info)
        );
        assert_eq!(parse_findings("[]"), Vec::new());
        assert_eq!(parse_findings("Looks good to me."), Vec::new());
    }
}
