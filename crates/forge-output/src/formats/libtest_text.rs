//! Rust's plain libtest output (`cargo test` on a stable toolchain, where `--format json`
//! needs `-Z unstable-options`):
//!
//! ```text
//!      Running unittests src/lib.rs (target/debug/deps/shop_core-49a7891832f2bb85)
//! running 3 tests
//! test cart::tests::vouchers ... ignored, needs the voucher service
//! test cart::tests::rounds_to_cents ... FAILED
//! test cart::tests::adds_items ... ok
//!
//! failures:
//!
//! ---- cart::tests::rounds_to_cents stdout ----
//! thread 'cart::tests::rounds_to_cents' panicked at crates/shop-core/src/cart.rs:69:9:
//! …
//! test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; …
//! ```
//!
//! - `running N tests` opens a suite named like the JSON parser's (cargo's `Running` /
//!   `Doc-tests` line, usually on stderr); `test result:` closes it.
//! - `test <name> ... ok | ignored[, reason] | FAILED | bench: …` are the results; a failed
//!   test is reported when its `---- <name> stdout ----` section has been read (at
//!   `test result:`), with the panic message, location and `assert_eq!` values.
//! - Locations follow [`crate::formats::libtest`]: `<crate>::<module path>`, doctests at
//!   their file and line.

use crate::formats::libtest::{cargo_binary_line, failure, test_location, Binary};
use crate::testevents::{LineParser, ParsedLine, TestEvent};

#[derive(Debug, Default)]
pub struct LibtestTextParser {
    next: Option<Binary>,
    suite: Option<String>,
    krate: Option<String>,
    count: u32,
    /// Failed tests waiting for their output section.
    failed: Vec<String>,
    /// The test whose `---- … stdout ----` section is being read, and its text.
    section: Option<(String, String)>,
    /// Output of failed tests, by name.
    outputs: Vec<(String, String)>,
    /// A test whose result follows on a later line (its own output came first).
    pending: Option<String>,
}

impl LibtestTextParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn close_section(&mut self) {
        if let Some((name, text)) = self.section.take() {
            self.outputs.push((name, text));
        }
    }

    /// Report the failed tests and close the suite.
    fn close_suite(&mut self) -> Vec<TestEvent> {
        self.close_section();
        let mut out = Vec::new();
        for name in std::mem::take(&mut self.failed) {
            let text = self
                .outputs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, t)| t.clone())
                .unwrap_or_default();
            out.extend(failure(&name, &text, false, None));
        }
        self.outputs.clear();
        if let Some(name) = self.suite.take().filter(|n| !n.is_empty()) {
            out.push(TestEvent::SuiteFinished { name });
        }
        out
    }

    fn started(&self, name: &str) -> TestEvent {
        TestEvent::TestStarted {
            name: name.to_string(),
            location: test_location(name, self.krate.as_deref()),
        }
    }

    /// The events of a result (`ok`, `FAILED`, `ignored, reason`, `bench: …`); `None` when
    /// the text is no result.
    fn result(&mut self, name: &str, result: &str) -> Option<Vec<TestEvent>> {
        let result = result.trim();
        let finished = || TestEvent::TestFinished {
            name: name.to_string(),
            duration: None,
        };
        if result == "ok" || result.starts_with("bench:") {
            return Some(vec![self.started(name), finished()]);
        }
        if result == "FAILED" {
            self.failed.push(name.to_string());
            return Some(vec![self.started(name)]);
        }
        if result == "ignored" || result.starts_with("ignored, ") {
            let message = result
                .strip_prefix("ignored, ")
                .map(str::to_string)
                .filter(|m| !m.is_empty());
            return Some(vec![TestEvent::TestIgnored {
                name: name.to_string(),
                message,
                location: test_location(name, self.krate.as_deref()),
            }]);
        }
        None
    }
}

/// `test <name> ... <result>` → (name, result).
fn test_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("test ")?;
    let (name, result) = rest.split_once(" ... ")?;
    // libtest appends ` - should panic` to the name of `#[should_panic]` tests.
    let name = name.strip_suffix(" - should panic").unwrap_or(name);
    Some((name, result))
}

impl LineParser for LibtestTextParser {
    fn stderr(&mut self, line: &str) {
        if let Some(b) = cargo_binary_line(line) {
            self.next = Some(b);
        }
    }

    fn line(&mut self, line: &str) -> ParsedLine {
        let trimmed = line.trim_end();
        if let Some(b) = cargo_binary_line(trimmed) {
            self.next = Some(b);
            return ParsedLine::console(line);
        }
        if let Some(n) = trimmed
            .strip_prefix("running ")
            .and_then(|r| r.strip_suffix(" tests").or_else(|| r.strip_suffix(" test")))
            .and_then(|n| n.trim().parse::<u32>().ok())
        {
            let mut out = self.close_suite();
            self.count += 1;
            let next = self.next.take();
            self.krate = next.as_ref().and_then(|b| b.krate.clone());
            let name = next.map(|b| b.suite).unwrap_or_else(|| {
                if self.count == 1 {
                    "tests".into()
                } else {
                    format!("tests ({})", self.count)
                }
            });
            // Binaries without tests (`running 0 tests`) get no suite.
            if n > 0 {
                out.push(TestEvent::Count { count: n });
                out.push(TestEvent::SuiteStarted {
                    name: name.clone(),
                    location: None,
                });
            }
            self.suite = Some(if n > 0 { name } else { String::new() });
            return ParsedLine::events(out);
        }
        if self.suite.is_none() {
            return ParsedLine::console(line);
        }
        if trimmed.starts_with("test result: ") {
            let mut parsed = ParsedLine::events(self.close_suite());
            parsed.console = Some(format!("{line}\n"));
            return parsed;
        }
        if let Some(name) = trimmed
            .strip_prefix("---- ")
            .and_then(|r| r.strip_suffix(" ----"))
        {
            self.close_section();
            let name = name
                .strip_suffix(" stdout")
                .or_else(|| name.strip_suffix(" stderr"))
                .unwrap_or(name);
            self.section = Some((name.to_string(), String::new()));
            return ParsedLine::console(line);
        }
        if trimmed == "failures:" || trimmed == "successes:" {
            self.close_section();
            return ParsedLine::console(line);
        }
        if let Some((_, text)) = self.section.as_mut() {
            text.push_str(trimmed);
            text.push('\n');
            return ParsedLine::console(line);
        }
        if let Some((name, result)) = test_line(trimmed) {
            if let Some(events) = self.result(name, result) {
                self.pending = None;
                return ParsedLine::events(events);
            }
            // The test printed before its result (`--nocapture`): the result comes later.
            self.pending = Some(name.to_string());
            let mut parsed = ParsedLine::events(vec![self.started(name)]);
            parsed.console = Some(format!("{}\n", result));
            return parsed;
        }
        if let Some(name) = self.pending.clone() {
            if let Some(mut events) = self.result(&name, trimmed) {
                self.pending = None;
                // Started was reported with the test line.
                events.retain(|e| !matches!(e, TestEvent::TestStarted { .. }));
                return ParsedLine::events(events);
            }
        }
        ParsedLine::console(line)
    }

    fn finish(&mut self) -> Vec<TestEvent> {
        self.close_suite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, run_lines, tree_of};
    use crate::teamcity::TestStatus;
    use crate::testevents::location;

    fn run_with_stderr(text: &str) -> Vec<TestEvent> {
        // cargo prints `Running` and `Doc-tests` on stderr.
        let mut p = LibtestTextParser::new();
        let mut out = Vec::new();
        for line in text.lines() {
            if cargo_binary_line(line).is_some() || line.trim_start().starts_with("error:") {
                p.stderr(line);
            } else {
                out.extend(p.line(line).events);
            }
        }
        out.extend(p.finish());
        out
    }

    #[test]
    fn plain_output_of_a_workspace() {
        let events = run_with_stderr(&fixture("libtest-plain.txt"));
        let tree = tree_of(&events);
        let rows: Vec<_> = tree
            .snapshot()
            .iter()
            .map(|n| (n.name.clone(), n.parent_id, n.status))
            .collect();
        let row = |name: &str, parent: u32, status| (name.to_string(), Some(parent), status);
        assert_eq!(
            rows,
            [
                ("Test Results".to_string(), None, TestStatus::Failed),
                row("src/lib.rs", 0, TestStatus::Failed),
                row("cart::tests::vouchers", 1, TestStatus::Ignored),
                row("cart::tests::rounds_to_cents", 1, TestStatus::Failed),
                row(
                    "money::tests::formats_negative_amounts",
                    1,
                    TestStatus::Passed
                ),
                row(
                    "money::tests::discounts_round_half_up",
                    1,
                    TestStatus::Passed
                ),
                row("cart::tests::adds_items", 1, TestStatus::Passed),
                row("tests/checkout.rs", 0, TestStatus::Passed),
                row("checkout_works", 7, TestStatus::Passed),
                row("Doc-tests shop_core", 0, TestStatus::Passed),
                row(
                    "crates/shop-core/src/lib.rs - price (line 18)",
                    9,
                    TestStatus::Passed
                ),
                row(
                    "crates/shop-core/src/cart.rs - cart::Cart::total (line 31)",
                    9,
                    TestStatus::Passed
                ),
                row(
                    "crates/shop-core/src/lib.rs - (line 3)",
                    9,
                    TestStatus::Passed
                ),
            ]
        );
        let nodes = tree.snapshot();
        let failed = &nodes[3];
        assert_eq!(
            failed.message.as_deref(),
            Some("assertion `left == right` failed")
        );
        assert_eq!(
            (failed.expected.as_deref(), failed.actual.as_deref()),
            (Some("588"), Some("589"))
        );
        assert_eq!(
            nodes[2].message.as_deref(),
            Some("needs the voucher service")
        );
        let ignored = events.iter().find_map(|e| match e {
            TestEvent::TestIgnored { location, .. } => location.clone(),
            _ => None,
        });
        assert_eq!(
            ignored,
            Some(location("shop_core::cart::tests", None, Some("vouchers")))
        );
    }

    #[test]
    fn json_and_plain_output_build_the_same_tree() {
        let json = run_lines(
            &mut crate::formats::LibtestParser::new(),
            &fixture("libtest-shop.jsonl"),
        );
        let text = run_with_stderr(&fixture("libtest-plain.txt"));
        let names = |events: &[TestEvent]| {
            let mut n: Vec<(String, TestStatus)> = tree_of(events)
                .snapshot()
                .into_iter()
                .map(|n| (n.name, n.status))
                .collect();
            n.sort_by(|a, b| a.0.cmp(&b.0));
            n
        };
        assert_eq!(names(&json), names(&text));
    }

    #[test]
    fn results_after_output_and_benchmarks() {
        let mut p = LibtestTextParser::new();
        let mut events = Vec::new();
        for l in [
            "running 3 tests",
            "test prints ... hello from the test",
            "ok",
            "test bench_total ... bench:         412.50 ns/iter (+/- 12.30)",
            "test panics - should panic ... ok",
            "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out",
        ] {
            events.extend(p.line(l).events);
        }
        let tree = tree_of(&events);
        assert_eq!(tree.counts().passed, 3, "{events:?}");
        let names: Vec<String> = tree.snapshot().into_iter().map(|n| n.name).collect();
        assert!(names.contains(&"panics".to_string()), "{names:?}");
    }
}
