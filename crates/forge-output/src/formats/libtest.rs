//! Rust's libtest JSON (`cargo test -- -Z unstable-options --format json --report-time`, or
//! nextest's libtest-json output): `{ "type": "suite" | "test", "event": … }` per line.
//!
//! - A `suite started` event opens a suite named after the test binary (the last
//!   `Running <target> (<binary>)` line cargo printed, else `tests`); `ok` / `failed` closes it.
//! - Test names are module paths (`cart::tests::rounds`); the location is
//!   `<crate>::<module path>` with the function as member, and a panic location
//!   (`panicked at src/cart.rs:42:9`) once the test fails.
//! - `assert_eq!` failures (`left: …`, `right: …`) fill expected (left) and actual (right).

use serde::Deserialize;

use crate::testevents::{location, LineParser, ParsedLine, TestEvent};

#[derive(Debug, Deserialize)]
struct LibtestEvent {
    #[serde(rename = "type")]
    kind: String,
    event: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    test_count: Option<u32>,
    #[serde(default)]
    exec_time: Option<f64>,
    #[serde(default)]
    stdout: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Default)]
pub struct LibtestParser {
    suite: Option<String>,
    /// From cargo's `Running unittests src/lib.rs (target/debug/deps/shop-…)` line.
    next_suite: Option<String>,
    count: u32,
}

/// `thread 'a::b' panicked at src/cart.rs:42:9:` → (`src/cart.rs`, 42).
fn panic_location(text: &str) -> Option<(String, u32)> {
    let at = text.find("panicked at ")? + "panicked at ".len();
    let rest = &text[at..];
    let rest = rest.trim_start_matches('\'');
    let end = rest.find([',', '\n', '\'']).unwrap_or(rest.len());
    let spot = rest[..end].trim_end_matches(':');
    let mut parts = spot.rsplitn(3, ':');
    let _col = parts.next()?;
    let line = parts.next()?.parse().ok()?;
    let file = parts.next()?;
    Some((file.to_string(), line))
}

/// The panic message: the text after `panicked at <spot>:` up to `note:` / the values.
fn panic_message(text: &str) -> Option<String> {
    let at = text.find("panicked at ")?;
    let after = &text[at..];
    let body = after.split_once('\n').map(|(_, b)| b).unwrap_or("");
    let msg: Vec<&str> = body
        .lines()
        .take_while(|l| !l.starts_with("note:") && !l.trim_start().starts_with("left:"))
        .collect();
    let m = msg.join("\n").trim().to_string();
    (!m.is_empty()).then_some(m)
}

fn assert_values(text: &str) -> Option<(String, String)> {
    let value = |key: &str| {
        text.lines().find_map(|l| {
            l.trim_start()
                .strip_prefix(key)
                .map(|v| v.trim().to_string())
        })
    };
    Some((value("left:")?, value("right:")?))
}

impl LibtestParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn suite_name(&mut self) -> String {
        self.count += 1;
        self.next_suite.take().unwrap_or_else(|| {
            if self.count == 1 {
                "tests".to_string()
            } else {
                format!("tests ({})", self.count)
            }
        })
    }
}

impl LineParser for LibtestParser {
    fn line(&mut self, line: &str) -> ParsedLine {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Running ") {
            // `Running unittests src/lib.rs (target/debug/deps/shop-1a2b)`, `Running tests/cart.rs (…)`.
            let name = rest.split(" (").next().unwrap_or(rest);
            self.next_suite = Some(name.trim_start_matches("unittests ").to_string());
            return ParsedLine::console(line);
        }
        let Some(e) = trimmed
            .starts_with('{')
            .then(|| serde_json::from_str::<LibtestEvent>(trimmed).ok())
            .flatten()
        else {
            return ParsedLine::console(line);
        };
        let mut out = Vec::new();
        let millis = e.exec_time.map(|s| (s * 1000.0).max(0.0).round() as u64);
        match (e.kind.as_str(), e.event.as_str()) {
            ("suite", "started") => {
                if let Some(s) = self.suite.take() {
                    out.push(TestEvent::SuiteFinished { name: s });
                }
                let name = self.suite_name();
                if let Some(n) = e.test_count {
                    out.push(TestEvent::Count { count: n });
                }
                out.push(TestEvent::SuiteStarted {
                    name: name.clone(),
                    location: None,
                });
                self.suite = Some(name);
            }
            ("suite", _) => {
                if let Some(s) = self.suite.take() {
                    out.push(TestEvent::SuiteFinished { name: s });
                }
            }
            ("test", "started") => {
                let name = e.name.unwrap_or_default();
                out.push(TestEvent::TestStarted {
                    location: name
                        .rsplit_once("::")
                        .map(|(path, member)| location(path, None, Some(member))),
                    name,
                });
            }
            ("test", "ok") => out.push(TestEvent::TestFinished {
                name: e.name.unwrap_or_default(),
                duration: millis,
            }),
            ("test", "failed") | ("test", "timeout") => {
                let name = e.name.unwrap_or_default();
                let text = e.stdout.or(e.message).unwrap_or_default();
                let (expected, actual) = assert_values(&text).unzip();
                out.push(TestEvent::TestFailed {
                    name: name.clone(),
                    message: panic_message(&text).unwrap_or_else(|| {
                        if e.event == "timeout" {
                            "Test timed out".into()
                        } else {
                            "Test failed".into()
                        }
                    }),
                    details: Some(text.trim_end().to_string()).filter(|t| !t.is_empty()),
                    expected,
                    actual,
                    location: panic_location(&text).map(|(f, l)| location(&f, Some(l), None)),
                });
                out.push(TestEvent::TestFinished {
                    name,
                    duration: millis,
                });
            }
            ("test", "ignored") => out.push(TestEvent::TestIgnored {
                name: e.name.unwrap_or_default(),
                message: e.message,
                location: None,
            }),
            _ => {}
        }
        ParsedLine::events(out)
    }

    fn finish(&mut self) -> Vec<TestEvent> {
        self.suite
            .take()
            .map(|name| TestEvent::SuiteFinished { name })
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, run_lines, tree_of};
    use crate::teamcity::TestStatus;

    #[test]
    fn suites_tests_and_panics() {
        let events = run_lines(&mut LibtestParser::new(), &fixture("libtest.jsonl"));
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
                ("src/lib.rs", Some(0), TestStatus::Failed),
                ("cart::tests::adds_items", Some(1), TestStatus::Passed),
                ("cart::tests::rounds_to_cents", Some(1), TestStatus::Failed),
                ("cart::tests::vouchers", Some(1), TestStatus::Ignored),
                ("tests/checkout.rs", Some(0), TestStatus::Passed),
                ("checkout_works", Some(5), TestStatus::Passed),
            ]
        );
        let failed = &nodes[3];
        assert_eq!(
            failed.message.as_deref(),
            Some("assertion `left == right` failed")
        );
        assert_eq!(failed.expected.as_deref(), Some("118"));
        assert_eq!(failed.actual.as_deref(), Some("117"));
        let loc = failed.location.as_ref().unwrap();
        assert_eq!(
            (loc.workspace_path.as_deref(), loc.line),
            (Some("src/cart.rs"), Some(42))
        );
        let loc = nodes[2].location.as_ref().unwrap();
        assert_eq!(
            (loc.path.as_str(), loc.member.as_deref()),
            ("cart::tests", Some("adds_items"))
        );
        assert_eq!(failed.duration, Some(2));
        assert_eq!(tree.summary().expected_count, Some(1));
    }

    #[test]
    fn panic_spots() {
        assert_eq!(
            panic_location("thread 'a' panicked at src/cart.rs:42:9:\nboom"),
            Some(("src/cart.rs".into(), 42))
        );
        assert_eq!(
            panic_location("thread 'a' panicked at 'boom', src/cart.rs:7:5"),
            None
        );
    }
}
