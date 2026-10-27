//! Test runner output formats turned into [`TestEvent`]s:
//!
//! | Format | Module | Arrives |
//! | --- | --- | --- |
//! | TeamCity service messages | [`crate::teamcity`] ([`TeamCityParser`]) | lines on stdout |
//! | JUnit XML | [`crate::junit`] | a document on stdout or report files |
//! | TAP 12–14 | [`tap`] | lines on stdout |
//! | Go `test2json` | [`gotest`] | lines on stdout |
//! | libtest JSON (`cargo test`) | [`libtest`] | lines on stdout |
//! | libtest plain text (`cargo test` on stable) | [`libtest_text`] | lines on stdout |
//! | pytest report log / JUnit | [`pytest`] | lines on stdout or a report file / report files |
//! | Jest / Vitest JSON | [`jest`] | a document on stdout or a report file |
//! | Playwright JSON | [`playwright`] | a report file |
//! | tox (environments as tests) | [`tox`] | lines on stdout |
//!
//! [`line_parser`] picks the line parser of a [`TestReport`]; [`document_events`] reads a
//! whole report (sniffing JSON, JSON lines and XML) for documents on stdout and report files.

pub mod gotest;
pub mod jest;
pub mod libtest;
pub mod libtest_text;
pub mod playwright;
pub mod pytest;
pub mod tap;
pub mod tox;

use crate::junit::{junit_messages, JUnitFlavor};
use crate::run::TestReport;
use crate::testevents::{LineParser, TeamCityParser, TestEvent};

pub use gotest::GoTestParser;
pub use jest::jest_events;
pub use libtest::LibtestParser;
pub use libtest_text::LibtestTextParser;
pub use playwright::playwright_events;
pub use pytest::PytestReportLogParser;
pub use tap::TapParser;
pub use tox::ToxParser;

/// The parser of a format that streams lines on stdout; `None` for whole documents.
pub fn line_parser(report: TestReport) -> Option<Box<dyn LineParser>> {
    Some(match report {
        TestReport::TeamCity => Box::new(TeamCityParser),
        TestReport::Tap => Box::new(TapParser::new()),
        TestReport::GoTestJson => Box::new(GoTestParser::new()),
        TestReport::LibtestJson => Box::new(LibtestParser::new()),
        TestReport::LibtestText => Box::new(LibtestTextParser::new()),
        TestReport::PytestReportLog => Box::new(PytestReportLogParser::new()),
        TestReport::Tox => Box::new(ToxParser::new()),
        TestReport::JUnitStdout | TestReport::JUnitFile | TestReport::JestJson => return None,
    })
}

/// The events of a whole report: JUnit XML, a Jest / Vitest JSON document or JSON lines
/// (a pytest report log, `test2json` or libtest output saved to a file).
pub fn document_events(text: &str, flavor: JUnitFlavor) -> Result<Vec<TestEvent>, String> {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with('<') {
        return Ok(junit_messages(t, flavor)?
            .iter()
            .filter_map(TestEvent::from_service_message)
            .collect());
    }
    if !t.starts_with('{') {
        return Err("not a test report".into());
    }
    if playwright::is_playwright_report(t) {
        if let Ok(events) = playwright_events(t) {
            return Ok(events);
        }
    }
    if let Ok(events) = jest_events(t) {
        return Ok(events);
    }
    // JSON lines: pick the parser by the first object.
    let first = t.lines().next().unwrap_or_default();
    let mut parser: Box<dyn LineParser> = if first.contains("\"$report_type\"") {
        Box::new(PytestReportLogParser::new())
    } else if first.contains("\"Action\"") {
        Box::new(GoTestParser::new())
    } else if first.contains("\"type\"") && first.contains("\"event\"") {
        Box::new(LibtestParser::new())
    } else {
        return Err("unknown JSON test report".into());
    };
    let mut out = Vec::new();
    for line in t.lines() {
        out.extend(parser.line(line).events);
    }
    out.extend(parser.finish());
    Ok(out)
}

/// Text without ANSI escape sequences (Jest and Vitest color their failure messages).
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    out
}

/// Remove the indentation all non-blank lines share.
pub fn dedent(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    text.lines()
        .map(|l| {
            if l.len() >= indent {
                &l[indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::links::LinkContext;
    use crate::teamcity::{LocationRule, TestStatus, TestTree};

    pub fn fixture(name: &str) -> String {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    pub fn run_lines(parser: &mut dyn LineParser, text: &str) -> Vec<TestEvent> {
        let mut out = Vec::new();
        for l in text.lines() {
            out.extend(parser.line(l).events);
        }
        out.extend(parser.finish());
        out
    }

    pub fn tree_with(events: &[TestEvent], rules: &[LocationRule]) -> TestTree {
        let mut t = TestTree::new("Test Results", LinkContext::new(Some("/p"), None))
            .with_location_rules(rules);
        for e in events {
            t.apply_event(e);
        }
        t.finish(Some(1));
        t
    }

    pub fn tree_of(events: &[TestEvent]) -> TestTree {
        tree_with(events, &[])
    }

    #[test]
    fn documents_are_sniffed() {
        let junit = document_events(&fixture("pytest-junit.xml"), JUnitFlavor::Pytest).unwrap();
        assert!(junit.iter().any(|e| e.name() == "test_rounds[half]"));
        let jest = document_events(&fixture("jest.json"), JUnitFlavor::Generic).unwrap();
        assert_eq!(tree_of(&jest).counts().total, 4);
        let pw = document_events(&fixture("playwright.json"), JUnitFlavor::Generic).unwrap();
        assert_eq!(tree_of(&pw).counts().total, 4);
        // Cypress (`--reporter junit`, Mocha's JUnit reporter).
        let cy = document_events(&fixture("cypress-junit.xml"), JUnitFlavor::Generic).unwrap();
        let tree = tree_of(&cy);
        assert_eq!((tree.counts().total, tree.counts().failed), (3, 1));
        let names: Vec<String> = tree.snapshot().into_iter().map(|n| n.name).collect();
        assert!(
            names.contains(&"Cart applies a voucher".to_string()),
            "{names:?}"
        );
        let log =
            document_events(&fixture("pytest-reportlog.jsonl"), JUnitFlavor::Generic).unwrap();
        assert_eq!(tree_of(&log).counts().failed, 1);
        let go = document_events(&fixture("go-test.jsonl"), JUnitFlavor::Generic).unwrap();
        assert_eq!(tree_of(&go).counts().total, 5);
        let rust: String = fixture("libtest.jsonl")
            .lines()
            .filter(|l| l.starts_with('{'))
            .map(|l| format!("{l}\n"))
            .collect();
        let rust = document_events(&rust, JUnitFlavor::Generic).unwrap();
        assert_eq!(tree_of(&rust).counts().total, 4);
        assert!(document_events("plain text", JUnitFlavor::Generic).is_err());
        let t = tree_of(&jest);
        assert_eq!(t.snapshot()[3].status, TestStatus::Passed);
    }

    #[test]
    fn strips_ansi_and_dedents() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[39m!"), "red!");
        assert_eq!(dedent("    a\n      b\n\n    c"), "a\n  b\n\nc");
    }
}
