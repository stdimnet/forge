//! Go `test2json` (`go test -json ./...`): one JSON object per line with `Action` (`start`,
//! `run`, `pause`, `cont`, `output`, `pass`, `fail`, `skip`, `bench`), `Package`, `Test`,
//! `Elapsed` (seconds) and `Output`.
//!
//! - Every package is a suite; tests (and subtests `TestCart/rounds`) are its tests.
//! - A test's output lines are its console; the console of the run shows them too.
//! - A failing test's message is the first `file_test.go:12: …` line of its output, which
//!   also gives the location (`<package>/<file>`; framework location rules map the import
//!   path to the project folder). Tests start with the location `<package>::<Test>`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::teamcity::TestLocation;
use crate::testevents::{location, LineParser, ParsedLine, TestEvent};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct GoEvent {
    action: String,
    #[serde(default)]
    package: String,
    #[serde(default)]
    test: Option<String>,
    #[serde(default)]
    elapsed: Option<f64>,
    #[serde(default)]
    output: Option<String>,
}

#[derive(Debug, Default)]
pub struct GoTestParser {
    /// The package whose suite is open.
    open: Option<String>,
    /// Output collected per running test (for the failure message).
    output: HashMap<String, String>,
}

/// `    cart_test.go:12: got 1.1781, want 1.18` → (`cart_test.go`, 12, `got 1.1781, want 1.18`).
fn failure_line(output: &str) -> Option<(String, u32, String)> {
    for line in output.lines() {
        let t = line.trim();
        let Some((file, rest)) = t.split_once(".go:") else {
            continue;
        };
        if file.contains(' ') {
            continue;
        }
        let (num, msg) = rest.split_once(':')?;
        let n: u32 = num.trim().parse().ok()?;
        return Some((format!("{file}.go"), n, msg.trim().to_string()));
    }
    None
}

/// `got X, want Y` (and `want Y, got X`) as (expected, actual).
fn got_want(message: &str) -> Option<(String, String)> {
    let m = message;
    let got = m.find("got ")?;
    let want = m.find("want ")?;
    let value = |start: usize| -> String {
        let s = &m[start..];
        s.split(", ").next().unwrap_or(s).trim().to_string()
    };
    Some((value(want + 5), value(got + 4)))
}

impl GoTestParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn open_package(&mut self, package: &str, out: &mut Vec<TestEvent>) {
        if self.open.as_deref() == Some(package) {
            return;
        }
        if let Some(p) = self.open.take() {
            out.push(TestEvent::SuiteFinished { name: p });
        }
        self.open = Some(package.to_string());
        out.push(TestEvent::SuiteStarted {
            name: package.to_string(),
            location: Some(location(package, None, None)),
        });
    }

    fn test_location(package: &str, test: &str) -> TestLocation {
        location(package, None, Some(test.split('/').next().unwrap_or(test)))
    }
}

impl LineParser for GoTestParser {
    fn line(&mut self, line: &str) -> ParsedLine {
        let Some(e) = line
            .trim_start()
            .starts_with('{')
            .then(|| serde_json::from_str::<GoEvent>(line.trim()).ok())
            .flatten()
        else {
            return ParsedLine::console(line);
        };
        let mut out = Vec::new();
        if !e.package.is_empty() && e.action != "output" && e.action != "start" {
            self.open_package(&e.package, &mut out);
        }
        let millis = e.elapsed.map(|s| (s * 1000.0).max(0.0).round() as u64);
        match (e.action.as_str(), e.test.as_deref()) {
            ("run", Some(t)) => {
                out.push(TestEvent::TestStarted {
                    name: t.to_string(),
                    location: Some(Self::test_location(&e.package, t)),
                });
            }
            ("output", Some(t)) => {
                let text = e.output.unwrap_or_default();
                // `=== RUN`, `--- PASS` lines only repeat what the tree shows.
                let trimmed = text.trim_start();
                if trimmed.starts_with("=== ") || trimmed.starts_with("--- ") {
                    return ParsedLine::events(out);
                }
                self.output
                    .entry(t.to_string())
                    .or_default()
                    .push_str(&text);
                out.push(TestEvent::TestOutput {
                    name: t.to_string(),
                    text: text.clone(),
                    stderr: false,
                });
                return ParsedLine {
                    events: out,
                    console: Some(text),
                    attributed: true,
                };
            }
            ("output", None) => {
                return ParsedLine {
                    events: out,
                    console: Some(e.output.unwrap_or_default()),
                    attributed: false,
                };
            }
            ("pass", Some(t)) | ("fail", Some(t)) => {
                let output = self.output.remove(t).unwrap_or_default();
                if e.action == "fail" {
                    let failure = failure_line(&output);
                    let message = failure
                        .as_ref()
                        .map(|(_, _, m)| m.clone())
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| "Test failed".to_string());
                    let (expected, actual) = got_want(&message).unzip();
                    out.push(TestEvent::TestFailed {
                        name: t.to_string(),
                        message,
                        details: Some(output.trim_end().to_string()).filter(|d| !d.is_empty()),
                        expected,
                        actual,
                        location: failure.map(|(file, line, _)| {
                            location(
                                &format!("{}/{file}", e.package.trim_end_matches('/')),
                                Some(line),
                                None,
                            )
                        }),
                    });
                }
                out.push(TestEvent::TestFinished {
                    name: t.to_string(),
                    duration: millis,
                });
            }
            ("skip", Some(t)) => {
                let output = self.output.remove(t).unwrap_or_default();
                let reason = failure_line(&output).map(|(_, _, m)| m);
                out.push(TestEvent::TestIgnored {
                    name: t.to_string(),
                    message: reason,
                    location: Some(Self::test_location(&e.package, t)),
                });
            }
            ("pass", None) | ("fail", None) | ("skip", None) => {
                if self.open.as_deref() == Some(e.package.as_str()) {
                    self.open = None;
                    out.push(TestEvent::SuiteFinished { name: e.package });
                }
            }
            _ => {}
        }
        ParsedLine::events(out)
    }

    fn finish(&mut self) -> Vec<TestEvent> {
        self.open
            .take()
            .map(|name| TestEvent::SuiteFinished { name })
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, run_lines, tree_with};
    use crate::teamcity::{LocationRule, TestStatus};

    #[test]
    fn packages_become_suites_with_failures_and_locations() {
        let mut parser = GoTestParser::new();
        let lines = fixture("go-test.jsonl");
        let mut console = String::new();
        for l in lines.lines() {
            if let Some(c) = parser.line(l).console {
                console.push_str(&c);
            }
        }
        assert!(console.contains("ok  \texample.com/shop/inventory"));
        assert!(console.contains("cart_test.go:14: got 1.1781, want 1.18"));
        let events = run_lines(&mut GoTestParser::new(), &lines);
        let tree = tree_with(
            &events,
            &[LocationRule {
                pattern: "^example\\.com/shop/?".into(),
                replacement: "".into(),
            }],
        );
        let nodes = tree.snapshot();
        let rows: Vec<_> = nodes
            .iter()
            .map(|n| (n.name.as_str(), n.parent_id, n.status))
            .collect();
        assert_eq!(
            rows,
            [
                ("Test Results", None, TestStatus::Failed),
                ("example.com/shop/cart", Some(0), TestStatus::Failed),
                ("TestAdd", Some(1), TestStatus::Passed),
                ("TestRound", Some(1), TestStatus::Failed),
                ("TestRound/half_up", Some(1), TestStatus::Failed),
                ("TestVoucher", Some(1), TestStatus::Ignored),
                ("example.com/shop/inventory", Some(0), TestStatus::Passed),
                ("TestReserve", Some(6), TestStatus::Passed),
            ]
        );
        let sub = &nodes[4];
        assert_eq!(sub.message.as_deref(), Some("got 1.1781, want 1.18"));
        assert_eq!(
            (sub.expected.as_deref(), sub.actual.as_deref()),
            (Some("1.18"), Some("1.1781"))
        );
        let loc = sub.location.as_ref().unwrap();
        assert_eq!(loc.workspace_path.as_deref(), Some("cart/cart_test.go"));
        assert_eq!(loc.line, Some(14));
        let loc = nodes[2].location.as_ref().unwrap();
        assert_eq!(loc.workspace_path.as_deref(), Some("cart"));
        assert_eq!(loc.member.as_deref(), Some("TestAdd"));
        assert_eq!(nodes[5].message.as_deref(), Some("needs a database"));
        assert_eq!(nodes[2].duration, Some(0));
        assert_eq!(nodes[3].duration, Some(10));
        let console: Vec<String> = nodes[4].console.iter().map(|l| l.text()).collect();
        assert!(console[0].contains("cart_test.go:14"));
    }

    #[test]
    fn reads_failure_lines() {
        assert_eq!(
            failure_line("    cart_test.go:14: got 1, want 2\n"),
            Some(("cart_test.go".into(), 14, "got 1, want 2".into()))
        );
        assert_eq!(failure_line("panic: boom"), None);
    }
}
