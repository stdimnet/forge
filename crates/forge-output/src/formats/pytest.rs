//! pytest results, from either of pytest's machine-readable reports:
//!
//! - **Report log** (`pytest --report-log=<file>`, the pytest-reportlog plugin): one JSON
//!   object per line (`$report_type`: `TestReport` per phase `setup` / `call` /
//!   `teardown`, `SessionFinish`). Parsed line by line, so it also works when the log goes
//!   to stdout next to pytest's own output.
//! - **JUnit XML** (`pytest --junitxml=<file>`): read by [`crate::junit`] with
//!   [`crate::JUnitFlavor::Pytest`], which uses [`module_file`] for the locations.
//!
//! Node ids (`tests/test_cart.py::TestCart::test_total[ten]`) give the suites (the file and
//! its classes), the test name and the location; `assert a == b` failures give the expected
//! (`b`) and actual (`a`) values.

use serde::Deserialize;
use serde_json::Value;

use crate::testevents::{location, LineParser, ParsedLine, TestEvent};

/// `tests.test_cart.TestCart` → (`tests/test_cart.py`, `Some("TestCart")`): the module
/// path up to the first capitalized segment is the file.
pub fn module_file(classname: &str) -> (String, Option<String>) {
    let parts: Vec<&str> = classname.split('.').filter(|p| !p.is_empty()).collect();
    let split = parts
        .iter()
        .position(|p| p.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
        .unwrap_or(parts.len());
    let file = format!("{}.py", parts[..split].join("/"));
    let class = (split < parts.len()).then(|| parts[split..].join("."));
    (file, class)
}

/// `assert 1.1781 == 1.18` (first line of a failure) → (expected `1.18`, actual `1.1781`).
pub fn assert_values(message: &str) -> Option<(String, String)> {
    let line = message.lines().map(str::trim).find_map(|l| {
        l.strip_prefix("assert ")
            .or_else(|| l.strip_prefix("AssertionError: assert "))
    })?;
    let (actual, expected) = line.split_once(" == ")?;
    Some((expected.trim().to_string(), actual.trim().to_string()))
}

/// `test_total[ten]` → `test_total`.
fn function_name(name: &str) -> &str {
    name.split('[').next().unwrap_or(name)
}

#[derive(Debug, Deserialize)]
struct Report {
    #[serde(rename = "$report_type")]
    report_type: String,
    #[serde(default)]
    nodeid: String,
    #[serde(default)]
    location: Option<(String, Option<u32>, String)>,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    when: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    longrepr: Value,
    #[serde(default)]
    sections: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub struct PytestReportLogParser {
    /// Open suites: the file, then its classes.
    open: Vec<String>,
    /// The call phase's duration of the running test.
    duration: Option<u64>,
}

/// The failure text and its first line of a `longrepr` (a string, a `[path, line, message]`
/// skip triple or pytest's serialized traceback).
fn longrepr_text(v: &Value) -> (String, Option<(String, u32)>) {
    match v {
        Value::String(s) => (s.clone(), None),
        Value::Array(a) => (
            a.get(2)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            None,
        ),
        Value::Object(o) => {
            let crash = o.get("reprcrash");
            let message = crash
                .and_then(|c| c.get("message"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let spot = crash.and_then(|c| {
                Some((
                    c.get("path")?.as_str()?.to_string(),
                    c.get("lineno")?.as_u64()? as u32,
                ))
            });
            let mut lines: Vec<String> = Vec::new();
            if let Some(entries) = o
                .get("reprtraceback")
                .and_then(|t| t.get("reprentries"))
                .and_then(Value::as_array)
            {
                for e in entries {
                    if let Some(ls) = e
                        .get("data")
                        .and_then(|d| d.get("lines"))
                        .and_then(Value::as_array)
                    {
                        lines.extend(ls.iter().filter_map(Value::as_str).map(str::to_string));
                    }
                }
            }
            let text = if lines.is_empty() {
                message.clone()
            } else {
                format!("{message}\n\n{}", lines.join("\n"))
            };
            (text, spot)
        }
        _ => (String::new(), None),
    }
}

impl PytestReportLogParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the file and class suites of a node id; returns the test name.
    fn enter(&mut self, nodeid: &str, out: &mut Vec<TestEvent>) -> String {
        let parts: Vec<&str> = nodeid.split("::").collect();
        let (test, suites) = parts.split_last().unwrap_or((&"", &[]));
        let common = self
            .open
            .iter()
            .zip(suites.iter())
            .take_while(|(a, b)| a == *b)
            .count();
        while self.open.len() > common {
            out.push(TestEvent::SuiteFinished {
                name: self.open.pop().unwrap_or_default(),
            });
        }
        let file = suites.first().copied().unwrap_or_default();
        for (i, s) in suites.iter().enumerate().skip(common) {
            out.push(TestEvent::SuiteStarted {
                name: s.to_string(),
                location: Some(if i == 0 {
                    location(file, None, None)
                } else {
                    location(file, None, Some(s))
                }),
            });
            self.open.push(s.to_string());
        }
        test.to_string()
    }
}

impl LineParser for PytestReportLogParser {
    fn line(&mut self, line: &str) -> ParsedLine {
        let t = line.trim();
        let Some(r) = (t.starts_with('{') && t.contains("\"$report_type\""))
            .then(|| serde_json::from_str::<Report>(t).ok())
            .flatten()
        else {
            return ParsedLine::console(line);
        };
        let mut out = Vec::new();
        match r.report_type.as_str() {
            "TestReport" => {
                let name = self.enter(&r.nodeid, &mut out);
                let file = r.nodeid.split("::").next().unwrap_or_default().to_string();
                let loc = location(
                    &file,
                    r.location.as_ref().and_then(|l| l.1).map(|l| l + 1),
                    Some(function_name(&name)),
                );
                let (text, spot) = longrepr_text(&r.longrepr);
                let millis = r.duration.map(|d| (d * 1000.0).max(0.0).round() as u64);
                let fail = |out: &mut Vec<TestEvent>, phase: &str| {
                    let first = text
                        .lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty())
                        .unwrap_or("Test failed")
                        .to_string();
                    let message = if phase == "call" {
                        first.clone()
                    } else {
                        format!("Error in {phase}: {first}")
                    };
                    let (expected, actual) = assert_values(&first).unzip();
                    out.push(TestEvent::TestFailed {
                        name: name.clone(),
                        message,
                        details: Some(text.trim_end().to_string()).filter(|d| !d.is_empty()),
                        expected,
                        actual,
                        location: spot.clone().map(|(p, l)| location(&p, Some(l), None)),
                    });
                };
                for (title, content) in &r.sections {
                    if title.starts_with("Captured") && !content.is_empty() {
                        out.push(TestEvent::TestOutput {
                            name: name.clone(),
                            text: content.clone(),
                            stderr: title.contains("stderr"),
                        });
                    }
                }
                match (r.when.as_str(), r.outcome.as_str()) {
                    ("setup", "skipped") => {
                        let reason = text.trim().trim_start_matches("Skipped: ").to_string();
                        out.push(TestEvent::TestIgnored {
                            name,
                            message: Some(reason).filter(|m| !m.is_empty()),
                            location: Some(loc),
                        });
                        // The skipped test has no call; its teardown is not reported.
                        self.duration = None;
                        return ParsedLine::events(out);
                    }
                    ("setup", outcome) => {
                        // Output sections came before the start: move them after it.
                        let outputs: Vec<TestEvent> = out
                            .iter()
                            .filter(|e| matches!(e, TestEvent::TestOutput { .. }))
                            .cloned()
                            .collect();
                        out.retain(|e| !matches!(e, TestEvent::TestOutput { .. }));
                        out.push(TestEvent::TestStarted {
                            name: name.clone(),
                            location: Some(loc),
                        });
                        out.extend(outputs);
                        if outcome == "failed" {
                            fail(&mut out, "setup");
                        }
                        self.duration = None;
                    }
                    ("call", outcome) => {
                        self.duration = millis;
                        match outcome {
                            "failed" => fail(&mut out, "call"),
                            "skipped" => out.push(TestEvent::TestIgnored {
                                name,
                                message: Some(
                                    text.trim().trim_start_matches("Skipped: ").to_string(),
                                )
                                .filter(|m| !m.is_empty()),
                                location: None,
                            }),
                            _ => {}
                        }
                    }
                    ("teardown", outcome) => {
                        if outcome == "failed" {
                            fail(&mut out, "teardown");
                        }
                        out.push(TestEvent::TestFinished {
                            name,
                            duration: self.duration.take(),
                        });
                    }
                    _ => {}
                }
            }
            "SessionFinish" => {
                while let Some(s) = self.open.pop() {
                    out.push(TestEvent::SuiteFinished { name: s });
                }
            }
            _ => {}
        }
        ParsedLine::events(out)
    }

    fn finish(&mut self) -> Vec<TestEvent> {
        let mut out = Vec::new();
        while let Some(s) = self.open.pop() {
            out.push(TestEvent::SuiteFinished { name: s });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, run_lines, tree_of};
    use crate::teamcity::TestStatus;

    #[test]
    fn module_paths_and_assertions() {
        assert_eq!(
            module_file("tests.test_cart.TestCart"),
            ("tests/test_cart.py".into(), Some("TestCart".into()))
        );
        assert_eq!(module_file("test_cart"), ("test_cart.py".into(), None));
        assert_eq!(
            assert_values("assert 1.1781 == 1.18"),
            Some(("1.18".into(), "1.1781".into()))
        );
        assert_eq!(assert_values("ValueError: x"), None);
    }

    #[test]
    fn report_logs_build_files_classes_and_tests() {
        let events = run_lines(
            &mut PytestReportLogParser::new(),
            &fixture("pytest-reportlog.jsonl"),
        );
        let tree = tree_of(&events);
        let nodes = tree.snapshot();
        let rows: Vec<_> = nodes
            .iter()
            .map(|n| (n.name.as_str(), n.parent_id, n.status))
            .collect();
        assert_eq!(
            rows,
            [
                ("Test Results", None, TestStatus::Failed),
                ("tests/test_cart.py", Some(0), TestStatus::Failed),
                ("TestCart", Some(1), TestStatus::Failed),
                ("test_adds_items", Some(2), TestStatus::Passed),
                ("test_rounds[half]", Some(2), TestStatus::Failed),
                ("test_vouchers", Some(1), TestStatus::Ignored),
            ]
        );
        let failed = &nodes[4];
        assert_eq!(failed.message.as_deref(), Some("assert 1.1781 == 1.18"));
        assert_eq!(failed.expected.as_deref(), Some("1.18"));
        assert_eq!(failed.actual.as_deref(), Some("1.1781"));
        assert_eq!(failed.duration, Some(2));
        let loc = failed.location.as_ref().unwrap();
        assert_eq!(
            (loc.workspace_path.as_deref(), loc.line),
            (Some("tests/test_cart.py"), Some(10))
        );
        assert_eq!(nodes[5].message.as_deref(), Some("needs a database"));
        let out: Vec<String> = nodes[3].console.iter().map(|l| l.text()).collect();
        assert_eq!(out, ["adding"]);
    }
}
