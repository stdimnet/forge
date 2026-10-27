//! RSpec's JSON report (`rspec --format progress --format json --out <file>`).
//!
//! - Every spec file (`examples[].file_path`, `./spec/cart_spec.rb`) is a suite; the example
//!   groups of an example (its `full_description` without its own `description`, e.g.
//!   `Cart #total`) form one nested suite, so the tree reads file → groups → example.
//! - `passed` passes, `failed` fails, `pending` (and `skipped`) is ignored with its
//!   `pending_message`. `run_time` is in seconds.
//! - The exception of a failed example gives the message, the backtrace as details, the
//!   `expected: … got: …` values of `eq` matchers ("Click to see difference") and the first
//!   backtrace frame in the spec file as the failure location.
//! - Tests carry their `full_description` as the location member, which is how the gutter
//!   keys examples (`<file>::<full description>`).
//! - Errors outside of examples (a spec file that does not load) become one failed test
//!   "RSpec could not load the specs" with RSpec's messages.

use serde::Deserialize;

use super::strip_ansi;
use crate::testevents::{location, TestEvent};

#[derive(Debug, Deserialize)]
struct Report {
    #[serde(default)]
    examples: Vec<Example>,
    #[serde(default)]
    messages: Vec<String>,
    #[serde(default)]
    summary: Option<Summary>,
}

#[derive(Debug, Deserialize)]
struct Summary {
    #[serde(default)]
    example_count: Option<u32>,
    #[serde(default)]
    errors_outside_of_examples_count: u32,
}

#[derive(Debug, Deserialize)]
struct Example {
    #[serde(default)]
    description: String,
    #[serde(default)]
    full_description: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    file_path: String,
    #[serde(default)]
    line_number: Option<u32>,
    #[serde(default)]
    run_time: Option<f64>,
    #[serde(default)]
    pending_message: Option<String>,
    #[serde(default)]
    exception: Option<Exception>,
}

#[derive(Debug, Deserialize)]
struct Exception {
    #[serde(default)]
    class: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    backtrace: Option<Vec<String>>,
}

/// Whether a JSON document is an RSpec report (`examples` and `summary_line`).
pub fn is_rspec_report(json: &str) -> bool {
    let head: String = json.chars().take(4096).collect();
    head.contains("\"examples\"")
        && (json.contains("\"summary_line\"") || head.contains("\"seed\""))
}

/// `./spec/cart_spec.rb` → `spec/cart_spec.rb`.
fn clean_path(p: &str) -> String {
    p.replace('\\', "/").trim_start_matches("./").to_string()
}

/// The example groups of an example: its full description without its own.
pub fn group_of(full: &str, description: &str) -> String {
    full.strip_suffix(description)
        .map(str::trim_end)
        .unwrap_or("")
        .to_string()
}

/// `expected: 1050\n     got: 1049` of RSpec's `eq` / `eql` / `equal` failures.
fn comparison(message: &str) -> (Option<String>, Option<String>) {
    let mut expected = None;
    let mut actual = None;
    for l in message.lines() {
        let t = l.trim_start();
        if let Some(v) = t.strip_prefix("expected:") {
            expected.get_or_insert_with(|| v.trim().to_string());
        } else if let Some(v) = t.strip_prefix("got:") {
            actual.get_or_insert_with(|| v.trim().to_string());
        }
    }
    if expected.is_some() && actual.is_some() {
        (expected, actual)
    } else {
        (None, None)
    }
}

/// `./spec/cart_spec.rb:14:in 'block (2 levels)…'` → line 14 when it is in `file`.
fn frame_line(backtrace: &[String], file: &str) -> Option<u32> {
    backtrace.iter().find_map(|f| {
        let f = f.trim_start_matches("./");
        let rest = f.strip_prefix(file)?.strip_prefix(':')?;
        rest.split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    })
}

/// The events of an RSpec JSON report, in tree order.
pub fn rspec_events(json: &str) -> Result<Vec<TestEvent>, String> {
    let report: Report =
        serde_json::from_str(json.trim()).map_err(|e| format!("not an RSpec report: {e}"))?;
    let mut out = Vec::new();
    if let Some(n) = report.summary.as_ref().and_then(|s| s.example_count) {
        out.push(TestEvent::Count { count: n });
    }
    // Files in the order of their first example.
    let mut files: Vec<String> = Vec::new();
    for e in &report.examples {
        let f = clean_path(&e.file_path);
        if !files.contains(&f) {
            files.push(f);
        }
    }
    for file in &files {
        let short = file.rsplit('/').next().unwrap_or(file).to_string();
        out.push(TestEvent::SuiteStarted {
            name: short.clone(),
            location: Some(location(file, None, None)),
        });
        let mut open: Option<String> = None;
        for e in report
            .examples
            .iter()
            .filter(|e| clean_path(&e.file_path) == *file)
        {
            let description = if e.description.is_empty() {
                e.full_description.clone()
            } else {
                e.description.clone()
            };
            let group = group_of(&e.full_description, &description);
            if open.as_deref() != Some(group.as_str()).filter(|g| !g.is_empty()) {
                if let Some(g) = open.take() {
                    out.push(TestEvent::SuiteFinished { name: g });
                }
                if !group.is_empty() {
                    out.push(TestEvent::SuiteStarted {
                        name: group.clone(),
                        location: Some(location(file, None, Some(&group))),
                    });
                    open = Some(group.clone());
                }
            }
            let full = if e.full_description.is_empty() {
                description.clone()
            } else {
                e.full_description.clone()
            };
            let loc = location(file, e.line_number, Some(&full));
            match e.status.as_str() {
                "pending" | "skipped" => {
                    out.push(TestEvent::TestIgnored {
                        name: description,
                        message: e.pending_message.clone().filter(|m| !m.is_empty()),
                        location: Some(loc),
                    });
                    continue;
                }
                _ => {}
            }
            out.push(TestEvent::TestStarted {
                name: description.clone(),
                location: Some(loc),
            });
            if e.status == "failed" {
                let ex = e.exception.as_ref();
                let text = strip_ansi(ex.map(|x| x.message.as_str()).unwrap_or(""));
                let message = text
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .map(str::to_string)
                    .or_else(|| ex.map(|x| x.class.clone()))
                    .unwrap_or_else(|| "Example failed".into());
                let backtrace = ex.and_then(|x| x.backtrace.clone()).unwrap_or_default();
                let mut details = text.trim_matches('\n').to_string();
                if let Some(class) = ex.map(|x| x.class.as_str()).filter(|c| !c.is_empty()) {
                    details = format!("{class}:\n{details}");
                }
                if !backtrace.is_empty() {
                    details.push_str("\n\n");
                    details.push_str(&backtrace.join("\n"));
                }
                let (expected, actual) = comparison(&text);
                out.push(TestEvent::TestFailed {
                    name: description.clone(),
                    message,
                    details: Some(details.trim().to_string()).filter(|d| !d.is_empty()),
                    expected,
                    actual,
                    location: frame_line(&backtrace, file).map(|l| location(file, Some(l), None)),
                });
            }
            out.push(TestEvent::TestFinished {
                name: description,
                duration: e.run_time.map(|s| (s.max(0.0) * 1000.0).round() as u64),
            });
        }
        if let Some(g) = open.take() {
            out.push(TestEvent::SuiteFinished { name: g });
        }
        out.push(TestEvent::SuiteFinished { name: short });
    }
    let outside = report
        .summary
        .as_ref()
        .map(|s| s.errors_outside_of_examples_count)
        .unwrap_or(0);
    if outside > 0 || (report.examples.is_empty() && !report.messages.is_empty()) {
        let name = "RSpec could not load the specs".to_string();
        let text = strip_ansi(&report.messages.join("\n"));
        out.push(TestEvent::TestStarted {
            name: name.clone(),
            location: None,
        });
        out.push(TestEvent::TestFailed {
            name: name.clone(),
            message: text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("An error occurred outside of examples")
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
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::document_events;
    use crate::formats::tests::{fixture, tree_of};
    use crate::junit::JUnitFlavor;
    use crate::teamcity::TestStatus;

    #[test]
    fn files_groups_and_examples() {
        let text = fixture("rspec.json");
        assert!(is_rspec_report(&text));
        let events = rspec_events(&text).unwrap();
        let tree = tree_of(&events);
        let rows: Vec<_> = tree
            .snapshot()
            .into_iter()
            .map(|n| (n.name, n.parent_id, n.status))
            .collect();
        assert_eq!(
            rows,
            [
                ("Test Results".to_string(), None, TestStatus::Failed),
                ("cart_spec.rb".into(), Some(0), TestStatus::Failed),
                ("Cart".into(), Some(1), TestStatus::Passed),
                ("adds items".into(), Some(2), TestStatus::Passed),
                ("Cart #total".into(), Some(1), TestStatus::Failed),
                ("sums the items".into(), Some(4), TestStatus::Passed),
                ("applies the voucher".into(), Some(4), TestStatus::Failed),
                ("Cart vouchers".into(), Some(1), TestStatus::Ignored),
                ("expire after a year".into(), Some(7), TestStatus::Ignored),
                ("money_spec.rb".into(), Some(0), TestStatus::Passed),
                ("Money".into(), Some(9), TestStatus::Passed),
                ("formats cents".into(), Some(10), TestStatus::Passed),
            ]
        );
        let failed = events
            .iter()
            .find_map(|e| match e {
                TestEvent::TestFailed {
                    message,
                    expected,
                    actual,
                    location,
                    details,
                    ..
                } => Some((message, expected, actual, location, details)),
                _ => None,
            })
            .unwrap();
        assert_eq!(failed.0, "expected: 1050");
        assert_eq!(failed.1.as_deref(), Some("1050"));
        assert_eq!(failed.2.as_deref(), Some("1049"));
        assert_eq!(failed.3.as_ref().unwrap().line, Some(18));
        assert!(failed
            .4
            .as_deref()
            .unwrap()
            .starts_with("RSpec::Expectations::ExpectationNotMetError"));
        // Examples carry their full description for the gutter keys.
        let started = events.iter().find_map(|e| match e {
            TestEvent::TestStarted { name, location } if name == "applies the voucher" => {
                location.clone()
            }
            _ => None,
        });
        let loc = started.unwrap();
        assert_eq!(loc.path, "spec/cart_spec.rb");
        assert_eq!(
            loc.member.as_deref(),
            Some("Cart #total applies the voucher")
        );
        assert_eq!(loc.line, Some(15));
        // The report is found among other JSON documents.
        let sniffed = document_events(&text, JUnitFlavor::Generic).unwrap();
        assert_eq!(sniffed, events);
    }

    #[test]
    fn load_errors_fail_the_run() {
        let json = r#"{"version":"3.13.0","messages":["\nAn error occurred while loading ./spec/broken_spec.rb.\nFailure/Error: require 'nope'\n\nLoadError:\n  cannot load such file -- nope"],"seed":1,"examples":[],"summary":{"duration":0.0,"example_count":0,"failure_count":0,"pending_count":0,"errors_outside_of_examples_count":1},"summary_line":"0 examples, 0 failures, 1 error occurred outside of examples"}"#;
        let tree = tree_of(&rspec_events(json).unwrap());
        assert_eq!(tree.counts().failed, 1);
        let node = tree.snapshot().into_iter().last().unwrap();
        assert_eq!(node.name, "RSpec could not load the specs");
    }

    #[test]
    fn groups_from_descriptions() {
        assert_eq!(group_of("Cart #total sums", "sums"), "Cart #total");
        assert_eq!(group_of("sums", "sums"), "");
        assert_eq!(group_of("other", "sums"), "");
    }
}
