//! The test event protocol every test runner output is turned into, whatever the tool
//! prints: TeamCity service messages, JUnit XML, TAP, Go `test2json`, libtest JSON, Jest /
//! Vitest JSON or pytest report logs (see [`crate::formats`]). [`crate::TestTree`] builds the
//! Run tool window's test tree from these events.
//!
//! Tests are addressed by name inside the suite that is open when they start (like TeamCity
//! messages). Locations are [`TestLocation`]s: a file (absolute, relative to the project or
//! as the tool reports it), an optional 1-based line and an optional member (class, method,
//! test function) used to find the line when the tool reports none.

use serde::{Deserialize, Serialize};

use crate::teamcity::{parse_location_hint, ServiceMessage, TestLocation};

/// One step of a test run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum TestEvent {
    /// The number of tests the run will execute.
    Count {
        count: u32,
    },
    /// A suite (file, class, package, describe block) starts; tests started afterwards
    /// belong to it until it finishes.
    #[serde(rename_all = "camelCase")]
    SuiteStarted {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<TestLocation>,
    },
    SuiteFinished {
        name: String,
    },
    #[serde(rename_all = "camelCase")]
    TestStarted {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<TestLocation>,
    },
    /// A failed test, with the expected and actual values of a comparison ("Click to see
    /// difference") when the tool reports them. `TestFinished` follows.
    #[serde(rename_all = "camelCase")]
    TestFailed {
        name: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actual: Option<String>,
        /// Where it failed (a stack frame), used when the test has no location yet.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<TestLocation>,
    },
    /// A skipped, pending or disabled test; it needs no start or finish.
    #[serde(rename_all = "camelCase")]
    TestIgnored {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<TestLocation>,
    },
    /// Output of a running test.
    #[serde(rename_all = "camelCase")]
    TestOutput {
        name: String,
        text: String,
        #[serde(default)]
        stderr: bool,
    },
    /// The test is done; tests without a failure passed. Milliseconds.
    #[serde(rename_all = "camelCase")]
    TestFinished {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<u64>,
    },
}

impl TestEvent {
    /// The TeamCity message as an event; `None` for messages that are not test events.
    pub fn from_service_message(msg: &ServiceMessage) -> Option<TestEvent> {
        let a = &msg.attrs;
        let name = || a.get("name").cloned().unwrap_or_default();
        let location = || parse_location_hint(a.get("locationHint").map(String::as_str));
        Some(match msg.name.as_str() {
            "testCount" => TestEvent::Count {
                count: a.get("count").and_then(|c| c.parse().ok()).unwrap_or(0),
            },
            "testSuiteStarted" => TestEvent::SuiteStarted {
                name: name(),
                location: location(),
            },
            "testSuiteFinished" => TestEvent::SuiteFinished { name: name() },
            "testStarted" => TestEvent::TestStarted {
                name: name(),
                location: location(),
            },
            "testFailed" => {
                let comparison = a.contains_key("expected") || a.contains_key("actual");
                TestEvent::TestFailed {
                    name: name(),
                    message: a.get("message").cloned().unwrap_or_default(),
                    details: a.get("details").cloned(),
                    expected: if comparison {
                        a.get("expected").cloned()
                    } else {
                        None
                    },
                    actual: if comparison {
                        a.get("actual").cloned()
                    } else {
                        None
                    },
                    location: None,
                }
            }
            "testIgnored" => TestEvent::TestIgnored {
                name: name(),
                message: a.get("message").cloned(),
                location: location(),
            },
            "testStdOut" | "testStdErr" => TestEvent::TestOutput {
                name: name(),
                text: a.get("out").cloned().unwrap_or_default(),
                stderr: msg.name == "testStdErr",
            },
            "testFinished" => TestEvent::TestFinished {
                name: name(),
                duration: a
                    .get("duration")
                    .and_then(|d| d.parse::<f64>().ok())
                    .map(|d| d.max(0.0).round() as u64),
            },
            _ => return None,
        })
    }

    /// The name of the test or suite the event is about (empty for `Count`).
    pub fn name(&self) -> &str {
        match self {
            TestEvent::Count { .. } => "",
            TestEvent::SuiteStarted { name, .. }
            | TestEvent::SuiteFinished { name }
            | TestEvent::TestStarted { name, .. }
            | TestEvent::TestFailed { name, .. }
            | TestEvent::TestIgnored { name, .. }
            | TestEvent::TestOutput { name, .. }
            | TestEvent::TestFinished { name, .. } => name,
        }
    }
}

/// What a line parser made of one line of output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedLine {
    pub events: Vec<TestEvent>,
    /// Text for the console (with its line break); `None` when the line was protocol only.
    pub console: Option<String>,
    /// The console text is output of a test the events already attribute
    /// ([`TestEvent::TestOutput`]); it must not be attributed to the running test again.
    pub attributed: bool,
}

impl ParsedLine {
    /// A line that is not part of the protocol: console text only.
    pub fn console(line: &str) -> Self {
        ParsedLine {
            events: Vec::new(),
            console: Some(format!("{line}\n")),
            attributed: false,
        }
    }

    pub fn events(events: Vec<TestEvent>) -> Self {
        ParsedLine {
            events,
            console: None,
            attributed: false,
        }
    }
}

/// A test runner format that arrives line by line on stdout (TeamCity, TAP, `test2json`,
/// libtest JSON, pytest report logs): protocol lines become events, other lines stay console
/// text. Lines come without their line break.
pub trait LineParser: Send + std::fmt::Debug {
    fn line(&mut self, line: &str) -> ParsedLine;
    /// The process ended: events still held back (a TAP failure waiting for diagnostics).
    fn finish(&mut self) -> Vec<TestEvent> {
        Vec::new()
    }
}

/// `##teamcity[…]` service messages.
#[derive(Debug, Default)]
pub struct TeamCityParser;

impl LineParser for TeamCityParser {
    fn line(&mut self, line: &str) -> ParsedLine {
        match crate::teamcity::parse_service_message(line) {
            Some(msg) => {
                ParsedLine::events(TestEvent::from_service_message(&msg).into_iter().collect())
            }
            None => ParsedLine::console(line),
        }
    }
}

/// `file`, `file:line` or `file::member` as a location.
pub fn location(path: &str, line: Option<u32>, member: Option<&str>) -> TestLocation {
    TestLocation {
        path: path.to_string(),
        workspace_path: None,
        member: member.filter(|m| !m.is_empty()).map(str::to_string),
        line: line.filter(|l| *l > 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teamcity::parse_service_message;

    #[test]
    fn teamcity_messages_become_events() {
        let ev = |l: &str| TestEvent::from_service_message(&parse_service_message(l).unwrap());
        assert_eq!(
            ev("##teamcity[testStarted name='a' locationHint='php_qn:///p/A.php::\\A::a']"),
            Some(TestEvent::TestStarted {
                name: "a".into(),
                location: Some(location("/p/A.php", None, Some("a"))),
            })
        );
        assert_eq!(
            ev("##teamcity[testFailed name='a' message='m' expected='1' actual='2']"),
            Some(TestEvent::TestFailed {
                name: "a".into(),
                message: "m".into(),
                details: None,
                expected: Some("1".into()),
                actual: Some("2".into()),
                location: None,
            })
        );
        assert_eq!(
            ev("##teamcity[testFinished name='a' duration='12.6']"),
            Some(TestEvent::TestFinished {
                name: "a".into(),
                duration: Some(13),
            })
        );
        assert_eq!(ev("##teamcity[progressMessage 'x']"), None);
        let json = serde_json::to_string(&ev("##teamcity[testCount count='3']").unwrap()).unwrap();
        assert_eq!(json, r#"{"event":"count","count":3}"#);
    }

    #[test]
    fn the_teamcity_parser_keeps_other_lines_for_the_console() {
        let mut p = TeamCityParser;
        assert_eq!(
            p.line("PHPUnit 11").console.as_deref(),
            Some("PHPUnit 11\n")
        );
        let parsed = p.line("##teamcity[testSuiteStarted name='S']");
        assert_eq!(parsed.console, None);
        assert_eq!(parsed.events[0].name(), "S");
    }
}
