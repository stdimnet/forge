//! Jest's JSON report (`jest --json [--outputFile=f] [--testLocationInResults]`), which
//! Vitest writes too (`vitest run --reporter=json [--outputFile=f]`).
//!
//! - Every test file (`testResults[].name`, absolute) is a suite, its `describe` blocks
//!   (`ancestorTitles`) are nested suites and `assertionResults` are the tests.
//! - `passed` passes, `failed` fails, `pending` / `skipped` / `todo` / `disabled` are
//!   ignored. Durations are milliseconds.
//! - Failure messages lose their ANSI colors; `Expected: …` / `Received: …` lines give the
//!   expected and actual values, the first stack frame in the test file the location.
//!   `location: { line, column }` (with `--testLocationInResults`) locates each test.
//! - A file that failed to run (a syntax error) fails its suite with the file's `message`.

use serde::Deserialize;

use super::strip_ansi;
use crate::testevents::{location, TestEvent};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    #[serde(default)]
    num_total_tests: Option<u32>,
    #[serde(default)]
    test_results: Vec<FileResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileResult {
    name: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    assertion_results: Vec<Assertion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Assertion {
    #[serde(default)]
    ancestor_titles: Vec<String>,
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    failure_messages: Vec<String>,
    #[serde(default)]
    location: Option<Position>,
}

#[derive(Debug, Deserialize)]
struct Position {
    line: u32,
}

fn value_after(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        l.trim_start()
            .strip_prefix(key)
            .map(|v| v.trim().to_string())
    })
}

/// `at Object.<anonymous> (/p/src/cart.test.ts:12:20)` in the file → line 12.
fn frame_line(text: &str, file: &str) -> Option<u32> {
    let at = text.find(&format!("{file}:"))? + file.len() + 1;
    text[at..]
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// The events of a Jest / Vitest JSON report, in tree order.
pub fn jest_events(json: &str) -> Result<Vec<TestEvent>, String> {
    let report: Report =
        serde_json::from_str(json).map_err(|e| format!("not a Jest report: {e}"))?;
    let mut out = Vec::new();
    if let Some(n) = report.num_total_tests {
        out.push(TestEvent::Count { count: n });
    }
    for file in &report.test_results {
        let file_name = file.name.replace('\\', "/");
        let short = file_name
            .rsplit('/')
            .next()
            .unwrap_or(&file_name)
            .to_string();
        out.push(TestEvent::SuiteStarted {
            name: short.clone(),
            location: Some(location(&file_name, None, None)),
        });
        let mut open: Vec<String> = Vec::new();
        for a in &file.assertion_results {
            let common = open
                .iter()
                .zip(&a.ancestor_titles)
                .take_while(|(x, y)| x == y)
                .count();
            while open.len() > common {
                out.push(TestEvent::SuiteFinished {
                    name: open.pop().unwrap_or_default(),
                });
            }
            for title in &a.ancestor_titles[common..] {
                out.push(TestEvent::SuiteStarted {
                    name: title.clone(),
                    location: Some(location(&file_name, None, Some(title))),
                });
                open.push(title.clone());
            }
            let loc = location(
                &file_name,
                a.location.as_ref().map(|p| p.line),
                Some(&a.title),
            );
            let name = a.title.clone();
            match a.status.as_str() {
                "pending" | "skipped" | "todo" | "disabled" => {
                    out.push(TestEvent::TestIgnored {
                        name,
                        message: (a.status == "todo").then(|| "todo".to_string()),
                        location: Some(loc),
                    });
                    continue;
                }
                _ => {}
            }
            out.push(TestEvent::TestStarted {
                name: name.clone(),
                location: Some(loc),
            });
            if a.status == "failed" {
                let text = strip_ansi(&a.failure_messages.join("\n"));
                let message = text
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("Test failed")
                    .to_string();
                let expected = value_after(&text, "Expected:");
                let actual = value_after(&text, "Received:");
                let failed_at = frame_line(&text, &file_name);
                out.push(TestEvent::TestFailed {
                    name: name.clone(),
                    message,
                    details: Some(text.trim_end().to_string()).filter(|t| !t.is_empty()),
                    expected: expected.clone().filter(|_| actual.is_some()),
                    actual: actual.filter(|_| expected.is_some()),
                    location: failed_at.map(|l| location(&file_name, Some(l), None)),
                });
            }
            out.push(TestEvent::TestFinished {
                name,
                duration: a.duration.map(|d| d.max(0.0).round() as u64),
            });
        }
        while let Some(s) = open.pop() {
            out.push(TestEvent::SuiteFinished { name: s });
        }
        if file.assertion_results.is_empty() && file.status == "failed" {
            // The file did not run: one failed entry with the error.
            let name = "Test suite failed to run".to_string();
            out.push(TestEvent::TestStarted {
                name: name.clone(),
                location: Some(location(&file_name, None, None)),
            });
            let text = strip_ansi(&file.message);
            out.push(TestEvent::TestFailed {
                name: name.clone(),
                message: text
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("Test suite failed to run")
                    .to_string(),
                details: Some(text.trim().to_string()).filter(|t| !t.is_empty()),
                expected: None,
                actual: None,
                location: None,
            });
            out.push(TestEvent::TestFinished {
                name,
                duration: None,
            });
        }
        out.push(TestEvent::SuiteFinished { name: short });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, tree_of};
    use crate::teamcity::TestStatus;

    #[test]
    fn files_describe_blocks_and_failures() {
        let events = jest_events(&fixture("jest.json")).unwrap();
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
                ("cart.test.ts", Some(0), TestStatus::Failed),
                ("Cart", Some(1), TestStatus::Failed),
                ("adds items", Some(2), TestStatus::Passed),
                ("totals", Some(2), TestStatus::Failed),
                ("rounds to cents", Some(4), TestStatus::Failed),
                ("applies vouchers", Some(2), TestStatus::Ignored),
                ("broken.test.ts", Some(0), TestStatus::Failed),
                ("Test suite failed to run", Some(7), TestStatus::Failed),
            ]
        );
        let failed = &nodes[5];
        assert_eq!(
            failed.message.as_deref(),
            Some("Error: expect(received).toBe(expected) // Object.is equality")
        );
        assert_eq!(failed.expected.as_deref(), Some("1.18"));
        assert_eq!(failed.actual.as_deref(), Some("1.1781"));
        let loc = failed.location.as_ref().unwrap();
        assert_eq!(loc.workspace_path.as_deref(), Some("src/cart.test.ts"));
        assert_eq!(loc.line, Some(14));
        assert_eq!(nodes[3].duration, Some(3));
        assert_eq!(tree.summary().expected_count, Some(4));
        assert!(jest_events("<xml/>").is_err());
    }
}
