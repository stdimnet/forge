//! The tools' machine-readable reports:
//!
//! | Tool | Output |
//! | --- | --- |
//! | PHPStan | `analyse --error-format=json` |
//! | Psalm | `--output-format=json` |
//! | PHP_CodeSniffer | `--report=json` |
//! | PHP Mess Detector | `phpmd <files> json <rulesets>` |
//! | PHP CS Fixer | `fix --dry-run --diff --format=json` |
//! | Laravel Pint | `--test --format=json` |
//! | Rector | `process --dry-run --output-format=json` |
//!
//! Text before or after the JSON (PHP notices, "Note: Using configuration file …") is
//! ignored. The fixers' reports become one problem per changed hunk, and their diffs are
//! kept so the fix can be applied or previewed.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diff;
use crate::model::{QualityTool, SeverityChoice};
use crate::problem::{PathContext, Problem, Severity};

/// What a fixer would change in one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFix {
    /// Project path.
    pub file: String,
    /// Unified diff (empty when the tool did not print one).
    pub diff: String,
    /// The fixers or rectors that apply (`braces_position`, `ReadOnlyPropertyRector`).
    pub rules: Vec<String>,
}

/// A parsed report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub problems: Vec<Problem>,
    pub fixes: Vec<FileFix>,
    /// Errors of the tool itself (a file it could not parse, a crashed worker).
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

/// Parse the report `tool` printed on stdout. `Err` when there is no JSON in it.
pub fn parse_report(
    tool: QualityTool,
    stdout: &str,
    ctx: &PathContext,
    severity: SeverityChoice,
) -> Result<Report, String> {
    let json =
        extract_json(stdout).ok_or_else(|| format!("{} printed no JSON report", tool.label()))?;
    let mut report = match tool {
        QualityTool::Phpstan => phpstan(&json, ctx),
        QualityTool::Psalm => psalm(&json, ctx),
        QualityTool::Phpcs => phpcs(&json, ctx),
        QualityTool::Phpmd => phpmd(&json, ctx),
        QualityTool::PhpCsFixer | QualityTool::Pint => cs_fixer(tool, &json, ctx),
        QualityTool::Rector => rector(&json, ctx),
    };
    for p in &mut report.problems {
        p.severity = p.severity.with_choice(severity);
    }
    Ok(report)
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn u32_of(v: &Value, key: &str) -> Option<u32> {
    v.get(key)
        .and_then(Value::as_u64)
        .map(|n| n.min(u32::MAX as u64) as u32)
}

/// A 1-based line or column as 0-based (0 and missing count as the first).
fn zero_based(n: Option<u32>) -> u32 {
    n.unwrap_or(1).saturating_sub(1)
}

fn problem(tool: QualityTool, file: String, line: u32, severity: Severity) -> Problem {
    Problem {
        tool,
        file,
        line,
        column: 0,
        end_line: None,
        end_column: None,
        severity,
        message: String::new(),
        rule: None,
        fixable: false,
        link: None,
    }
}

fn phpstan(json: &Value, ctx: &PathContext) -> Report {
    let mut r = Report::default();
    if let Some(files) = json.get("files").and_then(Value::as_object) {
        for (file, entry) in files {
            let file = ctx.localize(file);
            for m in entry
                .get("messages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let mut p = problem(
                    QualityTool::Phpstan,
                    file.clone(),
                    zero_based(u32_of(m, "line")),
                    Severity::Error,
                );
                p.message = str_of(m, "message").unwrap_or_default().to_string();
                if let Some(tip) = str_of(m, "tip").filter(|t| !t.is_empty()) {
                    p.message = format!("{}\nTip: {tip}", p.message);
                }
                p.rule = str_of(m, "identifier").map(str::to_string);
                r.problems.push(p);
            }
        }
    }
    r.errors = json
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| e.as_str().map(str::to_string))
        .collect();
    r
}

fn psalm(json: &Value, ctx: &PathContext) -> Report {
    let mut r = Report::default();
    for m in json.as_array().into_iter().flatten() {
        let Some(file) = str_of(m, "file_path").or_else(|| str_of(m, "file_name")) else {
            continue;
        };
        let severity = match str_of(m, "severity") {
            Some("error") => Severity::Error,
            Some("info") => Severity::Weak,
            _ => Severity::Warning,
        };
        let mut p = problem(
            QualityTool::Psalm,
            ctx.localize(file),
            zero_based(u32_of(m, "line_from")),
            severity,
        );
        p.column = zero_based(u32_of(m, "column_from"));
        p.end_line = u32_of(m, "line_to").map(|l| l.saturating_sub(1));
        p.end_column = u32_of(m, "column_to").map(|c| c.saturating_sub(1));
        let kind = str_of(m, "type").unwrap_or_default();
        p.message = str_of(m, "message").unwrap_or_default().to_string();
        p.rule = Some(kind.to_string()).filter(|k| !k.is_empty());
        p.link = str_of(m, "link").map(str::to_string);
        r.problems.push(p);
    }
    r
}

fn phpcs(json: &Value, ctx: &PathContext) -> Report {
    let mut r = Report::default();
    for (file, entry) in json
        .get("files")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let file = ctx.localize(file);
        for m in entry
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let severity = match str_of(m, "type") {
                Some("ERROR") => Severity::Error,
                _ => Severity::Warning,
            };
            let mut p = problem(
                QualityTool::Phpcs,
                file.clone(),
                zero_based(u32_of(m, "line")),
                severity,
            );
            p.column = zero_based(u32_of(m, "column"));
            p.message = str_of(m, "message").unwrap_or_default().to_string();
            p.rule = str_of(m, "source").map(str::to_string);
            p.fixable = m.get("fixable").and_then(Value::as_bool).unwrap_or(false);
            r.problems.push(p);
        }
    }
    r
}

fn phpmd(json: &Value, ctx: &PathContext) -> Report {
    let mut r = Report::default();
    for f in json
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(file) = str_of(f, "file") else {
            continue;
        };
        let file = ctx.localize(file);
        for v in f
            .get("violations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            // PHPMD priorities: 1 (highest) … 5.
            let severity = match u32_of(v, "priority").unwrap_or(3) {
                0..=2 => Severity::Warning,
                _ => Severity::Weak,
            };
            let mut p = problem(
                QualityTool::Phpmd,
                file.clone(),
                zero_based(u32_of(v, "beginLine")),
                severity,
            );
            p.message = str_of(v, "description")
                .unwrap_or_default()
                .trim()
                .to_string();
            p.rule = str_of(v, "rule").map(str::to_string);
            p.link = str_of(v, "externalInfoUrl")
                .filter(|u| u.starts_with("http"))
                .map(str::to_string);
            r.problems.push(p);
        }
    }
    for e in json
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let msg = str_of(e, "message").unwrap_or_default();
        r.errors.push(match str_of(e, "fileName") {
            Some(f) => format!("{}: {msg}", ctx.localize(f)),
            None => msg.to_string(),
        });
    }
    r
}

/// Problems at the hunks a fixer would change.
fn fix_problems(tool: QualityTool, fix: &FileFix, message: &str) -> Vec<Problem> {
    let hunks = diff::parse(&fix.diff);
    let at = |line: usize, len: usize| {
        let mut p = problem(tool, fix.file.clone(), line as u32, Severity::Weak);
        if len > 1 {
            p.end_line = Some((line + len - 1) as u32);
        }
        p.message = message.to_string();
        p.rule = Some(fix.rules.join(", ")).filter(|r| !r.is_empty());
        p.fixable = true;
        p
    };
    if hunks.is_empty() {
        return vec![at(0, 1)];
    }
    hunks
        .iter()
        .map(|h| {
            let (line, removed) = h.changed_lines();
            at(line, removed)
        })
        .collect()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect()
}

fn cs_fixer(tool: QualityTool, json: &Value, ctx: &PathContext) -> Report {
    let mut r = Report::default();
    for f in json
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = str_of(f, "name") else {
            continue;
        };
        let fix = FileFix {
            file: ctx.localize(name),
            diff: str_of(f, "diff").unwrap_or_default().to_string(),
            rules: strings(f.get("appliedFixers")),
        };
        let message = if fix.rules.is_empty() {
            format!("{}: the code style can be fixed", tool.label())
        } else {
            format!("{}: {}", tool.label(), fix.rules.join(", "))
        };
        r.problems.extend(fix_problems(tool, &fix, &message));
        r.fixes.push(fix);
    }
    r
}

/// `Rector\Php80\Rector\Class_\ClassPropertyAssignToConstructorPromotionRector` →
/// `ClassPropertyAssignToConstructorPromotionRector`.
pub fn short_rule(rule: &str) -> &str {
    rule.rsplit('\\').next().unwrap_or(rule)
}

fn rector(json: &Value, ctx: &PathContext) -> Report {
    let mut r = Report::default();
    for f in json
        .get("file_diffs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = str_of(f, "file") else {
            continue;
        };
        let fix = FileFix {
            file: ctx.localize(name),
            diff: str_of(f, "diff").unwrap_or_default().to_string(),
            rules: strings(f.get("applied_rectors")),
        };
        let short: Vec<&str> = fix.rules.iter().map(|r| short_rule(r)).collect();
        let message = format!("Rector: {}", short.join(", "));
        r.problems
            .extend(fix_problems(QualityTool::Rector, &fix, &message));
        r.fixes.push(fix);
    }
    for e in json
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let msg = str_of(e, "message").unwrap_or_default().to_string();
        match str_of(e, "file") {
            Some(file) => {
                let mut p = problem(
                    QualityTool::Rector,
                    ctx.localize(file),
                    zero_based(u32_of(e, "line")),
                    Severity::Error,
                );
                p.message = format!("Rector: {msg}");
                r.problems.push(p);
            }
            None => r.errors.push(msg),
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_json_between_other_output() {
        let v =
            extract_json("Note: Using configuration file /p/phpstan.neon.\n{\"a\": [1]}\ntrailing")
                .unwrap();
        assert_eq!(v["a"][0], 1);
        assert!(extract_json("PHP Fatal error: x in [file] {oops").is_none());
    }

    #[test]
    fn severity_choice_overrides_the_tools() {
        let out = r#"{"totals":{},"files":{"/p/a.php":{"errors":1,"messages":[{"message":"m","line":3}]}},"errors":[]}"#;
        let r = parse_report(
            QualityTool::Phpstan,
            out,
            &PathContext::local("/p"),
            SeverityChoice::Weak,
        )
        .unwrap();
        assert_eq!(r.problems[0].severity, Severity::Weak);
        assert_eq!(r.problems[0].line, 2);
        assert!(parse_report(
            QualityTool::Psalm,
            "",
            &PathContext::local("/p"),
            SeverityChoice::Tool
        )
        .is_err());
    }
}
