//! Rust's libtest JSON (`cargo test -- -Z unstable-options --format json --report-time`, or
//! nextest's libtest-json output): `{ "type": "suite" | "test", "event": … }` per line.
//!
//! - A `suite started` event opens a suite named after the test binary: the last
//!   `Running <target> (<binary>)` or `Doc-tests <crate>` line cargo printed (on stderr,
//!   see [`LineParser::stderr`]), else `tests`; `ok` / `failed` closes it.
//! - Test names are module paths (`cart::tests::rounds`); the location is
//!   `<crate>::<module path>` with the function as member (the crate from the binary's file
//!   name, `target/debug/deps/shop_core-1a2b` → `shop_core`), so location rules of the
//!   Cargo project can map modules to files. A panic location (`panicked at
//!   src/cart.rs:42:9`) is added once the test fails.
//! - Doctests (`src/lib.rs - cart::Cart::total (line 31)`) are located at their file and
//!   line, with the documented item as member.
//! - nextest names tests `<binary id>$<test name>` (`shop-core::checkout$checkout_works`);
//!   the binary id names the suite and the crate.
//! - `assert_eq!` failures (`left: …`, `right: …`) fill expected (left) and actual (right).
//!
//! [`crate::formats::libtest_text`] reads the plain output of stable toolchains with the same
//! helpers.

use serde::Deserialize;

use crate::teamcity::TestLocation;
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
    #[serde(default)]
    nextest: Option<NextestSuite>,
}

/// nextest's suite details (`"nextest": { "crate": "shop-core", "test_binary": "checkout" }`).
#[derive(Debug, Deserialize)]
struct NextestSuite {
    #[serde(default, rename = "crate")]
    krate: String,
    #[serde(default)]
    test_binary: String,
}

/// The test binary cargo announced and the crate its tests belong to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Binary {
    /// Suite name: `src/lib.rs`, `tests/checkout.rs`, `Doc-tests shop_core`.
    pub suite: String,
    /// Crate name as Rust paths spell it (`shop_core`).
    pub krate: Option<String>,
}

/// `Running unittests src/lib.rs (target/debug/deps/shop_core-1a2b)` or `Doc-tests shop_core`.
pub(crate) fn cargo_binary_line(line: &str) -> Option<Binary> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("Running ") {
        let (target, binary) = match rest.split_once(" (") {
            Some((t, b)) => (t, Some(b.trim_end_matches(')'))),
            None => (rest, None),
        };
        let krate = binary.and_then(|b| {
            let file = b.rsplit(['/', '\\']).next().unwrap_or(b);
            let file = file.strip_suffix(".exe").unwrap_or(file);
            let stem = match file.rsplit_once('-') {
                Some((stem, hash)) if hash.chars().all(|c| c.is_ascii_hexdigit()) => stem,
                _ => file,
            };
            (!stem.is_empty()).then(|| stem.replace('-', "_"))
        });
        return Some(Binary {
            suite: target.trim_start_matches("unittests ").to_string(),
            krate,
        });
    }
    let krate = trimmed.strip_prefix("Doc-tests ")?.trim();
    Some(Binary {
        suite: trimmed.to_string(),
        krate: Some(krate.replace('-', "_")),
    })
}

/// A doctest name: `src/lib.rs - cart::Cart::total (line 31)` → (`src/lib.rs`,
/// `cart::Cart::total`, 31); the crate's own docs have no item (`src/lib.rs - (line 3)`).
pub(crate) fn doctest_name(name: &str) -> Option<(String, String, u32)> {
    let (file, rest) = name.split_once(" - ")?;
    if !file.ends_with(".rs") {
        return None;
    }
    let at = rest.find("(line ")?;
    let line: u32 = rest[at + 6..].split(')').next()?.trim().parse().ok()?;
    Some((file.to_string(), rest[..at].trim().to_string(), line))
}

/// Where a test lives: doctests at their file and line, other tests at
/// `<crate>::<module path>` with the function as member.
pub(crate) fn test_location(name: &str, krate: Option<&str>) -> Option<TestLocation> {
    if let Some((file, item, line)) = doctest_name(name) {
        let member = item.rsplit("::").next().unwrap_or("").to_string();
        return Some(location(&file, Some(line), Some(&member)));
    }
    let qualify = |module: &str| match (krate, module.is_empty()) {
        (Some(k), true) => k.to_string(),
        (Some(k), false) => format!("{k}::{module}"),
        (None, _) => module.to_string(),
    };
    match name.rsplit_once("::") {
        Some((module, member)) => Some(location(&qualify(module), None, Some(member))),
        None if krate.is_some() => Some(location(&qualify(""), None, Some(name))),
        None => None,
    }
}

/// `thread 'a::b' panicked at src/cart.rs:42:9:` → (`src/cart.rs`, 42).
pub(crate) fn panic_location(text: &str) -> Option<(String, u32)> {
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
pub(crate) fn panic_message(text: &str) -> Option<String> {
    let at = text.find("panicked at ")?;
    let after = &text[at..];
    let body = after.split_once('\n').map(|(_, b)| b).unwrap_or("");
    let msg: Vec<&str> = body
        .lines()
        .take_while(|l| {
            !l.starts_with("note:")
                && !l.starts_with("stack backtrace:")
                && !l.trim_start().starts_with("left:")
        })
        .collect();
    let m = msg.join("\n").trim().to_string();
    (!m.is_empty()).then_some(m)
}

pub(crate) fn assert_values(text: &str) -> Option<(String, String)> {
    let value = |key: &str| {
        text.lines().find_map(|l| {
            l.trim_start()
                .strip_prefix(key)
                .map(|v| v.trim().to_string())
        })
    };
    Some((value("left:")?, value("right:")?))
}

/// The events of a failed test with its captured output.
pub(crate) fn failure(
    name: &str,
    text: &str,
    timeout: bool,
    millis: Option<u64>,
) -> [TestEvent; 2] {
    let (expected, actual) = assert_values(text).unzip();
    [
        TestEvent::TestFailed {
            name: name.to_string(),
            message: panic_message(text).unwrap_or_else(|| {
                if timeout {
                    "Test timed out".into()
                } else {
                    "Test failed".into()
                }
            }),
            details: Some(text.trim_end().to_string()).filter(|t| !t.is_empty()),
            expected,
            actual,
            location: panic_location(text).map(|(f, l)| location(&f, Some(l), None)),
        },
        TestEvent::TestFinished {
            name: name.to_string(),
            duration: millis,
        },
    ]
}

#[derive(Debug, Default)]
pub struct LibtestParser {
    suite: Option<String>,
    /// From cargo's `Running unittests src/lib.rs (target/debug/deps/shop-…)` line.
    next: Option<Binary>,
    /// The crate of the open suite.
    krate: Option<String>,
    count: u32,
}

impl LibtestParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn open_suite(&mut self, nextest: Option<&NextestSuite>) -> String {
        self.count += 1;
        let next = self.next.take();
        if let Some(n) = nextest.filter(|n| !n.krate.is_empty()) {
            let binary = if n.test_binary.is_empty() || n.test_binary == n.krate {
                n.krate.clone()
            } else {
                format!("{}::{}", n.krate, n.test_binary)
            };
            self.krate = Some(nextest_crate(&binary));
            return binary;
        }
        self.krate = next.as_ref().and_then(|b| b.krate.clone());
        next.map(|b| b.suite).unwrap_or_else(|| {
            if self.count == 1 {
                "tests".to_string()
            } else {
                format!("tests ({})", self.count)
            }
        })
    }

    /// The test name and crate of a reported name (nextest prefixes the binary id).
    fn split_name(&self, name: &str) -> (String, Option<String>) {
        match name.split_once('$') {
            Some((binary, test)) => (test.to_string(), Some(nextest_crate(binary))),
            None => (name.to_string(), self.krate.clone()),
        }
    }
}

/// The crate of a nextest binary id: `shop-core` → `shop_core`, `shop-core::checkout` →
/// `checkout`, `shop-cli::bin/shop` → `shop`.
fn nextest_crate(binary: &str) -> String {
    let last = binary.rsplit("::").next().unwrap_or(binary);
    let last = last.rsplit('/').next().unwrap_or(last);
    last.replace('-', "_")
}

impl LineParser for LibtestParser {
    fn stderr(&mut self, line: &str) {
        if let Some(b) = cargo_binary_line(line) {
            self.next = Some(b);
        }
    }

    fn line(&mut self, line: &str) -> ParsedLine {
        let trimmed = line.trim();
        if let Some(b) = cargo_binary_line(trimmed) {
            self.next = Some(b);
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
        let (name, krate) = self.split_name(e.name.as_deref().unwrap_or_default());
        match (e.kind.as_str(), e.event.as_str()) {
            ("suite", "started") => {
                if let Some(s) = self.suite.take().filter(|s| !s.is_empty()) {
                    out.push(TestEvent::SuiteFinished { name: s });
                }
                let suite = self.open_suite(e.nextest.as_ref());
                // Binaries without tests get no suite.
                if e.test_count == Some(0) {
                    self.suite = Some(String::new());
                } else {
                    if let Some(n) = e.test_count {
                        out.push(TestEvent::Count { count: n });
                    }
                    out.push(TestEvent::SuiteStarted {
                        name: suite.clone(),
                        location: None,
                    });
                    self.suite = Some(suite);
                }
            }
            ("suite", _) => {
                if let Some(s) = self.suite.take().filter(|s| !s.is_empty()) {
                    out.push(TestEvent::SuiteFinished { name: s });
                }
            }
            ("test", "started") => out.push(TestEvent::TestStarted {
                location: test_location(&name, krate.as_deref()),
                name,
            }),
            ("test", "ok") => out.push(TestEvent::TestFinished {
                name,
                duration: millis,
            }),
            ("test", "failed") | ("test", "timeout") => {
                let text = e.stdout.or(e.message).unwrap_or_default();
                out.extend(failure(&name, &text, e.event == "timeout", millis));
            }
            ("test", "ignored") => out.push(TestEvent::TestIgnored {
                location: test_location(&name, krate.as_deref()),
                name,
                message: e.message,
            }),
            _ => {}
        }
        ParsedLine::events(out)
    }

    fn finish(&mut self) -> Vec<TestEvent> {
        self.suite
            .take()
            .filter(|s| !s.is_empty())
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
            ("shop::cart::tests", Some("adds_items"))
        );
        // Tests in the crate root are located at the crate.
        let loc = nodes[6].location.as_ref().unwrap();
        assert_eq!(
            (loc.path.as_str(), loc.member.as_deref()),
            ("checkout", Some("checkout_works"))
        );
        assert_eq!(failed.duration, Some(2));
        assert_eq!(tree.summary().expected_count, Some(1));
    }

    #[test]
    fn cargo_lines_on_stderr_name_suites_and_doctests_have_files() {
        let mut p = LibtestParser::new();
        p.stderr(
            "     Running unittests src/lib.rs (target/debug/deps/shop_core-49a7891832f2bb85)",
        );
        let mut events = Vec::new();
        for l in [
            r#"{ "type": "suite", "event": "started", "test_count": 1 }"#,
            r#"{ "type": "test", "event": "started", "name": "money::tests::formats" }"#,
            r#"{ "type": "test", "name": "money::tests::formats", "event": "ok" }"#,
            r#"{ "type": "suite", "event": "ok" }"#,
        ] {
            events.extend(p.line(l).events);
        }
        p.stderr("   Doc-tests shop_core");
        for l in [
            r#"{ "type": "suite", "event": "started", "test_count": 2 }"#,
            r#"{ "type": "test", "event": "started", "name": "crates/shop-core/src/cart.rs - cart::Cart::total (line 31)" }"#,
            r#"{ "type": "test", "event": "started", "name": "crates/shop-core/src/lib.rs - (line 3)" }"#,
        ] {
            events.extend(p.line(l).events);
        }
        let started: Vec<(String, Option<TestLocation>)> = events
            .iter()
            .filter_map(|e| match e {
                TestEvent::SuiteStarted { name, .. } => Some((name.clone(), None)),
                TestEvent::TestStarted { name, location } => Some((name.clone(), location.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(started[0].0, "src/lib.rs");
        assert_eq!(
            started[1].1,
            Some(location("shop_core::money::tests", None, Some("formats")))
        );
        assert_eq!(started[2].0, "Doc-tests shop_core");
        assert_eq!(
            started[3].1,
            Some(location(
                "crates/shop-core/src/cart.rs",
                Some(31),
                Some("total")
            ))
        );
        assert_eq!(
            started[4].1,
            Some(location("crates/shop-core/src/lib.rs", Some(3), None))
        );
    }

    #[test]
    fn nextest_names_carry_their_binary() {
        let mut p = LibtestParser::new();
        let mut events = Vec::new();
        for l in [
            r#"{"type":"suite","event":"started","test_count":1,"nextest":{"crate":"shop-core","test_binary":"checkout","kind":"test"}}"#,
            r#"{"type":"test","event":"started","name":"shop-core::checkout$checkout_works"}"#,
            r#"{"type":"test","event":"ok","name":"shop-core::checkout$checkout_works","exec_time":0.001}"#,
            r#"{"type":"suite","event":"ok","passed":1,"failed":0,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.001}"#,
        ] {
            events.extend(p.line(l).events);
        }
        assert_eq!(
            events[1],
            TestEvent::SuiteStarted {
                name: "shop-core::checkout".into(),
                location: None
            }
        );
        assert_eq!(
            events[2],
            TestEvent::TestStarted {
                name: "checkout_works".into(),
                location: Some(location("checkout", None, Some("checkout_works")))
            }
        );
    }

    #[test]
    fn panic_spots() {
        assert_eq!(
            panic_location("thread 'a' panicked at src/cart.rs:42:9:\nboom"),
            Some(("src/cart.rs".into(), 42))
        );
        assert_eq!(
            panic_location("thread 'a' (3081) panicked at crates/x/src/cart.rs:69:9:\nboom"),
            Some(("crates/x/src/cart.rs".into(), 69))
        );
        assert_eq!(
            panic_location("thread 'a' panicked at 'boom', src/cart.rs:7:5"),
            None
        );
        assert_eq!(
            cargo_binary_line("Running tests/checkout.rs (target/debug/deps/checkout-aff1435.exe)"),
            Some(Binary {
                suite: "tests/checkout.rs".into(),
                krate: Some("checkout".into())
            })
        );
        assert_eq!(
            doctest_name("src/lib.rs - price (line 18) - compile fail"),
            Some(("src/lib.rs".into(), "price".into(), 18))
        );
        assert_eq!(doctest_name("cart::tests::x"), None);
    }
}
