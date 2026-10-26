//! Playwright Test's JSON report (`playwright test --reporter=json`, written to the file in
//! `PLAYWRIGHT_JSON_OUTPUT_NAME` or printed on stdout).
//!
//! - Every test file (`suites[]`, `file` relative to `config.rootDir`) is a suite, its
//!   `test.describe` blocks (nested `suites`) are nested suites and `specs` are the tests.
//! - A spec runs once per project (`chromium`, `firefox`); with several projects each run
//!   is its own test, named `<title> [<project>]`.
//! - `status`: `expected` and `flaky` (passed on a retry) pass, `unexpected` fails,
//!   `skipped` is ignored. The duration is the sum of all attempts, in milliseconds.
//! - Failure messages lose their ANSI colors; `Expected…: …` / `Received…: …` lines give
//!   the expected and actual values, the error's `location` the failure line.

use serde::Deserialize;

use super::strip_ansi;
use crate::testevents::{location, TestEvent};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    #[serde(default)]
    config: Config,
    suites: Vec<Suite>,
    #[serde(default)]
    errors: Vec<Error>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    #[serde(default)]
    root_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Suite {
    title: String,
    #[serde(default)]
    file: String,
    #[serde(default)]
    line: u32,
    #[serde(default)]
    specs: Vec<Spec>,
    #[serde(default)]
    suites: Vec<Suite>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    title: String,
    #[serde(default)]
    line: u32,
    #[serde(default)]
    tests: Vec<Test>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Test {
    #[serde(default)]
    project_name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    results: Vec<Attempt>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attempt {
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    error: Option<Error>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Error {
    #[serde(default)]
    message: String,
    #[serde(default)]
    stack: String,
    #[serde(default)]
    location: Option<ErrorLocation>,
}

#[derive(Debug, Deserialize)]
struct ErrorLocation {
    file: String,
    line: u32,
}

/// The value after `Expected…:` / `Received…:` (`Expected string: "a"` → `"a"`).
fn value_after(text: &str, word: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let t = l.trim_start();
        let rest = t.strip_prefix(word)?;
        let (label, value) = rest.split_once(':')?;
        (label.chars().all(|c| c.is_alphabetic() || c == ' ')).then(|| value.trim().to_string())
    })
}

fn join(root: &str, file: &str) -> String {
    let file = file.replace('\\', "/");
    if root.is_empty() || file.starts_with('/') || file.get(1..2) == Some(":") {
        return file;
    }
    format!("{}/{file}", root.replace('\\', "/").trim_end_matches('/'))
}

struct Walker {
    root: String,
    multi_project: bool,
    out: Vec<TestEvent>,
}

impl Walker {
    fn suite(&mut self, suite: &Suite, file: &str) {
        for spec in &suite.specs {
            for test in &spec.tests {
                self.test(spec, test, file);
            }
        }
        for child in &suite.suites {
            let path = join(
                &self.root,
                if child.file.is_empty() {
                    file
                } else {
                    &child.file
                },
            );
            self.out.push(TestEvent::SuiteStarted {
                name: child.title.clone(),
                location: Some(location(&path, Some(child.line), Some(&child.title))),
            });
            self.suite(child, &path);
            self.out.push(TestEvent::SuiteFinished {
                name: child.title.clone(),
            });
        }
    }

    fn test(&mut self, spec: &Spec, test: &Test, file: &str) {
        let name = if self.multi_project && !test.project_name.is_empty() {
            format!("{} [{}]", spec.title, test.project_name)
        } else {
            spec.title.clone()
        };
        let loc = location(file, Some(spec.line), Some(&spec.title));
        if test.status == "skipped" {
            self.out.push(TestEvent::TestIgnored {
                name,
                message: None,
                location: Some(loc),
            });
            return;
        }
        self.out.push(TestEvent::TestStarted {
            name: name.clone(),
            location: Some(loc),
        });
        if test.status == "unexpected" {
            let error = test.results.iter().rev().find_map(|r| r.error.as_ref());
            let text = error
                .map(|e| {
                    strip_ansi(if e.message.is_empty() {
                        &e.stack
                    } else {
                        &e.message
                    })
                })
                .unwrap_or_default();
            let expected = value_after(&text, "Expected");
            let actual = value_after(&text, "Received");
            let stack = error.map(|e| strip_ansi(&e.stack)).unwrap_or_default();
            let details = if stack.is_empty() || text.contains(stack.trim()) {
                text.clone()
            } else {
                format!("{text}\n{stack}")
            };
            self.out.push(TestEvent::TestFailed {
                name: name.clone(),
                message: text
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("Test failed")
                    .to_string(),
                details: Some(details.trim_end().to_string()).filter(|t| !t.is_empty()),
                expected: expected.clone().filter(|_| actual.is_some()),
                actual: actual.filter(|_| expected.is_some()),
                location: error
                    .and_then(|e| e.location.as_ref())
                    .map(|l| location(&l.file.replace('\\', "/"), Some(l.line), None)),
            });
        }
        let duration: f64 = test.results.iter().map(|r| r.duration.max(0.0)).sum();
        self.out.push(TestEvent::TestFinished {
            name,
            duration: Some(duration.round() as u64),
        });
    }
}

/// Whether a JSON document looks like a Playwright report.
pub fn is_playwright_report(json: &str) -> bool {
    json.contains("\"suites\"") && json.contains("\"specs\"")
}

/// The events of a Playwright JSON report, in tree order.
pub fn playwright_events(json: &str) -> Result<Vec<TestEvent>, String> {
    let report: Report =
        serde_json::from_str(json).map_err(|e| format!("not a Playwright report: {e}"))?;
    let mut projects: Vec<&str> = Vec::new();
    fn collect<'a>(s: &'a Suite, out: &mut Vec<&'a str>) {
        for t in s.specs.iter().flat_map(|sp| &sp.tests) {
            if !out.contains(&t.project_name.as_str()) {
                out.push(&t.project_name);
            }
        }
        for c in &s.suites {
            collect(c, out);
        }
    }
    for s in &report.suites {
        collect(s, &mut projects);
    }
    let mut w = Walker {
        root: report.config.root_dir.clone(),
        multi_project: projects.len() > 1,
        out: Vec::new(),
    };
    for file in &report.suites {
        let path = join(
            &w.root,
            if file.file.is_empty() {
                &file.title
            } else {
                &file.file
            },
        );
        let short = path.rsplit('/').next().unwrap_or(&path).to_string();
        w.out.push(TestEvent::SuiteStarted {
            name: short.clone(),
            location: Some(location(&path, None, None)),
        });
        w.suite(file, &path);
        w.out.push(TestEvent::SuiteFinished { name: short });
    }
    // Errors outside tests (a broken config, a file that does not compile).
    for (i, e) in report.errors.iter().enumerate() {
        let name = if i == 0 {
            "Test run failed".to_string()
        } else {
            format!("Test run failed ({})", i + 1)
        };
        let text = strip_ansi(if e.message.is_empty() {
            &e.stack
        } else {
            &e.message
        });
        w.out.push(TestEvent::TestStarted {
            name: name.clone(),
            location: None,
        });
        w.out.push(TestEvent::TestFailed {
            name: name.clone(),
            message: text.lines().next().unwrap_or("Error").trim().to_string(),
            details: Some(text.trim().to_string()).filter(|t| !t.is_empty()),
            expected: None,
            actual: None,
            location: e
                .location
                .as_ref()
                .map(|l| location(&l.file.replace('\\', "/"), Some(l.line), None)),
        });
        w.out.push(TestEvent::TestFinished {
            name,
            duration: None,
        });
    }
    Ok(w.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, tree_of};
    use crate::teamcity::TestStatus;

    #[test]
    fn files_describe_blocks_projects_and_failures() {
        let events = playwright_events(&fixture("playwright.json")).unwrap();
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
                ("checkout.spec.ts", Some(0), TestStatus::Failed),
                ("shows the home page", Some(1), TestStatus::Passed),
                ("Checkout", Some(1), TestStatus::Failed),
                ("adds the voucher", Some(3), TestStatus::Failed),
                ("keeps the cart", Some(3), TestStatus::Ignored),
                ("retries the payment", Some(3), TestStatus::Passed),
            ]
        );
        let failed = &nodes[4];
        assert_eq!(
            failed.message.as_deref(),
            Some("Error: expect(locator).toHaveText(expected)")
        );
        assert_eq!(failed.expected.as_deref(), Some("\"Total: 9.00\""));
        assert_eq!(failed.actual.as_deref(), Some("\"Total: 10.00\""));
        let loc = failed.location.as_ref().unwrap();
        assert_eq!(loc.line, Some(8), "the test, not the failure line");
        assert_eq!(nodes[6].duration, Some(500), "both attempts");
        assert_eq!(nodes[2].location.as_ref().unwrap().line, Some(3));
        assert!(playwright_events("{}").is_err());
    }

    #[test]
    fn several_projects_name_their_runs() {
        let json = r#"{"config":{"rootDir":"/p/e2e"},"suites":[{"title":"a.spec.ts","file":"a.spec.ts",
            "specs":[{"title":"loads","line":2,"tests":[
              {"projectName":"chromium","status":"expected","results":[{"duration":5}]},
              {"projectName":"firefox","status":"expected","results":[{"duration":7}]}]}]}],
            "errors":[{"message":"Error: config broken"}]}"#;
        let tree = tree_of(&playwright_events(json).unwrap());
        let names: Vec<String> = tree.snapshot().into_iter().map(|n| n.name).collect();
        assert_eq!(
            names,
            [
                "Test Results",
                "a.spec.ts",
                "loads [chromium]",
                "loads [firefox]",
                "Test run failed"
            ]
        );
    }
}
