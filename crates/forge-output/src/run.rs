//! The output of one run: console lines for every stream, and for test runners the test
//! tree built from TeamCity service messages on stdout (those lines are not shown in the
//! console). Output while a test runs is attributed to that test.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::ansi::{AnsiParser, Style};
use crate::console::{ConsoleDelta, ConsoleModel};
use crate::formats::{document_events, line_parser};
use crate::junit::JUnitFlavor;
use crate::links::{LinkContext, OutputPathMapping};
use crate::teamcity::{LocationRule, TestNode, TestSummary, TestTree};
use crate::testevents::{LineParser, TestEvent};

/// Where a test run reports its results, and in which format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TestReport {
    /// TeamCity service messages on stdout (PHPUnit `--teamcity`, Pest, Jest reporters).
    #[default]
    TeamCity,
    /// A JUnit XML document on stdout (PHPSpec `--format=junit`): stdout is collected and
    /// turned into the tree when the process exits.
    JUnitStdout,
    /// Report files the runner writes (JUnit XML of Codeception `--xml`, Behat
    /// `--format=junit`, pytest `--junitxml`; Jest / Vitest JSON; pytest report logs): the
    /// console shows stdout, the backend hands the files over as [`RunInput::Report`].
    JUnitFile,
    /// TAP on stdout (busted `-o TAP`, node `--test-reporter=tap`, prove, Bats).
    Tap,
    /// Go `test2json` lines on stdout (`go test -json`).
    GoTestJson,
    /// libtest JSON lines on stdout (`cargo test -- -Z unstable-options --format json`).
    LibtestJson,
    /// libtest's plain output on stdout (`cargo test` on a stable toolchain).
    LibtestText,
    /// A pytest report log on stdout (`--report-log=/dev/stdout`).
    PytestReportLog,
    /// A Jest / Vitest JSON document on stdout (`jest --json`, `vitest --reporter=json`),
    /// read when the process exits.
    JestJson,
    /// tox's output on stdout: every environment is a test (`tox -e py312,lint`).
    Tox,
}

impl TestReport {
    /// Formats that print one whole document on stdout, read at the exit.
    pub fn is_stdout_document(self) -> bool {
        matches!(self, TestReport::JUnitStdout | TestReport::JestJson)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutputOptions {
    /// Build a test tree from TeamCity messages on stdout.
    #[serde(default)]
    pub test_runner: bool,
    /// Project root, to make file links and test locations workspace-relative.
    #[serde(default)]
    pub root: Option<String>,
    /// Working directory of the process (absolute or relative to the root).
    #[serde(default)]
    pub cwd: Option<String>,
    /// Console cycle buffer: the oldest lines are dropped while the console holds more text
    /// than this (Settings | Editor | General | Console). `None` keeps everything.
    #[serde(default)]
    pub cycle_buffer_kb: Option<usize>,
    /// Paths where the process runs (a container, an SSH host) and their local
    /// counterparts, so links in the output open project files.
    #[serde(default)]
    pub path_mappings: Vec<OutputPathMapping>,
    /// External Tools output filters (`$FILE_PATH$:$LINE$`); invalid ones are ignored.
    #[serde(default)]
    pub output_filters: Vec<String>,
    /// How a test runner reports results (with `test_runner`).
    #[serde(default)]
    pub test_report: TestReport,
    /// The tool that writes JUnit reports (locations are derived differently).
    #[serde(default)]
    pub junit_flavor: JUnitFlavor,
    /// Rewrites of reported test locations (a test framework's location rules).
    #[serde(default)]
    pub location_rules: Vec<LocationRule>,
    /// The file a single-file test run tests (tests without a location belong to it).
    #[serde(default)]
    pub default_test_file: Option<String>,
}

/// What happened to the process.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunInput {
    #[serde(rename_all = "camelCase")]
    Started { command_line: String },
    /// `stream` is `stdout` or `stderr`.
    Output { stream: String, text: String },
    /// Text the user typed into the console (echoed).
    Input { text: String },
    /// A JUnit XML report written by the test runner, read after it exited (before
    /// [`RunInput::Exited`]).
    Report { xml: String },
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
}

/// What the UI renders, emitted on `run://<runId>` next to the raw events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OutputEvent {
    Console(ConsoleDelta),
    /// Changed test nodes (parents first) and the status line.
    Tests {
        nodes: Vec<TestNode>,
        summary: TestSummary,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug)]
pub struct RunOutput {
    ansi: AnsiParser,
    console: ConsoleModel,
    tests: Option<TestTree>,
    report: TestReport,
    flavor: JUnitFlavor,
    /// The line parser of a streaming format.
    parser: Option<Box<dyn LineParser>>,
    /// Stdout of a run that prints one document ([`TestReport::is_stdout_document`]),
    /// parsed when it exits.
    junit: String,
    /// Incomplete stdout line of a test run (service messages are line based).
    partial: String,
    /// Incomplete stderr line, for the line parser's view of stderr.
    stderr_partial: String,
    current_test: Option<usize>,
    delta: ConsoleDelta,
    tests_changed: bool,
    /// Cycle buffer size in bytes, the text size of every line the UI holds and their sum.
    limit: Option<usize>,
    sizes: VecDeque<usize>,
    total: usize,
}

impl ConsoleDelta {
    /// Append a later delta to this one.
    fn then(&mut self, next: ConsoleDelta) {
        if next.lines.is_empty() {
            return;
        }
        if next.replace_last {
            if self.lines.is_empty() {
                self.replace_last = true;
            } else {
                self.lines.pop();
            }
        }
        self.lines.extend(next.lines);
    }
}

impl RunOutput {
    pub fn new(opts: &RunOutputOptions) -> Self {
        let links = LinkContext::new(opts.root.as_deref(), opts.cwd.as_deref())
            .with_mappings(opts.path_mappings.clone())
            .with_filters(crate::filters::compile_filters(&opts.output_filters).0);
        RunOutput {
            ansi: AnsiParser::new(),
            console: ConsoleModel::new(links.clone()),
            tests: opts.test_runner.then(|| {
                TestTree::new("Test Results", links)
                    .with_location_rules(&opts.location_rules)
                    .with_default_file(opts.default_test_file.clone())
            }),
            report: opts.test_report,
            flavor: opts.junit_flavor,
            parser: opts
                .test_runner
                .then(|| line_parser(opts.test_report))
                .flatten(),
            junit: String::new(),
            partial: String::new(),
            stderr_partial: String::new(),
            current_test: None,
            delta: ConsoleDelta::default(),
            tests_changed: false,
            limit: opts.cycle_buffer_kb.map(|kb| kb.max(1) * 1024),
            sizes: VecDeque::new(),
            total: 0,
        }
    }

    /// Track the lines of `delta` and drop the oldest ones beyond the cycle buffer.
    fn cycle(&mut self, delta: &mut ConsoleDelta) {
        let Some(limit) = self.limit else {
            return;
        };
        if delta.replace_last && !delta.lines.is_empty() {
            if let Some(last) = self.sizes.pop_back() {
                self.total -= last;
            }
        }
        for line in &delta.lines {
            let size = line.segments.iter().map(|s| s.text.len()).sum::<usize>() + 1;
            self.sizes.push_back(size);
            self.total += size;
        }
        // The newest line always stays.
        while self.total > limit && self.sizes.len() > 1 {
            self.total -= self.sizes.pop_front().unwrap_or(0);
            delta.drop_front += 1;
        }
    }

    pub fn handle(&mut self, input: &RunInput) -> Vec<OutputEvent> {
        match input {
            RunInput::Started { command_line } => {
                self.write(&format!("{command_line}\n"), Stream::System)
            }
            RunInput::Output { stream, text } => {
                let stream = if stream == "stderr" {
                    Stream::Stderr
                } else {
                    Stream::Stdout
                };
                if self.tests.is_some()
                    && stream == Stream::Stdout
                    && self.report.is_stdout_document()
                {
                    self.junit.push_str(text);
                } else if self.parser.is_some() && stream == Stream::Stdout {
                    self.write_test_output(text, false);
                } else {
                    if stream == Stream::Stderr {
                        self.observe_stderr(text);
                    }
                    self.write(text, stream);
                }
            }
            RunInput::Input { text } => self.write(text, Stream::Stdout),
            RunInput::Report { xml } => {
                self.apply_report(xml);
            }
            RunInput::Exited { code, signal } => {
                if self.parser.is_some() {
                    if !self.partial.is_empty() {
                        self.write_test_output("", true);
                    }
                    let rest = self.parser.as_mut().map(|p| p.finish()).unwrap_or_default();
                    self.apply_events(&rest);
                }
                if self.tests.is_some() && !self.junit.is_empty() {
                    let all = std::mem::take(&mut self.junit);
                    // Anything before the document (warnings, deprecations) is console text.
                    let start = if self.report == TestReport::JestJson {
                        if all.trim_start().starts_with('{') {
                            Some(all.len() - all.trim_start().len())
                        } else {
                            all.find("\n{").map(|i| i + 1)
                        }
                    } else {
                        all.find("<?xml").or_else(|| all.find("<testsuites"))
                    }
                    .unwrap_or(all.len());
                    if start > 0 {
                        self.write(&all[..start], Stream::Stdout);
                    }
                    if start < all.len() && !self.apply_report(&all[start..]) {
                        self.write(&all[start..], Stream::Stdout);
                    }
                }
                if let Some(tree) = &mut self.tests {
                    tree.finish(*code);
                    self.tests_changed = true;
                }
                self.current_test = None;
                let how = match (code, signal) {
                    (Some(c), _) => format!("exit code {c}"),
                    (None, Some(s)) => format!(
                        "exit code {} (interrupted by signal {s}{})",
                        128 + s,
                        match s {
                            15 => ": SIGTERM",
                            9 => ": SIGKILL",
                            _ => "",
                        }
                    ),
                    (None, None) => "an unknown exit code".to_string(),
                };
                self.write(&format!("\nProcess finished with {how}\n"), Stream::System);
            }
        }
        self.take_events()
    }

    fn take_events(&mut self) -> Vec<OutputEvent> {
        let mut out = Vec::new();
        let mut delta = std::mem::take(&mut self.delta);
        self.cycle(&mut delta);
        if !delta.is_empty() {
            out.push(OutputEvent::Console(delta));
        }
        if let Some(tree) = &mut self.tests {
            let nodes = tree.take_updates();
            if !nodes.is_empty() || std::mem::take(&mut self.tests_changed) {
                out.push(OutputEvent::Tests {
                    nodes,
                    summary: tree.summary(),
                });
            }
        }
        out
    }

    fn write(&mut self, text: &str, stream: Stream) {
        self.write_attributed(text, stream, true);
    }

    /// Console text; `attribute` adds it to the output of the running test.
    fn write_attributed(&mut self, text: &str, stream: Stream, attribute: bool) {
        let style = match stream {
            Stream::Stderr => Some(Style::fg("var(--danger)")),
            Stream::System => Some(Style::fg("var(--fg-muted)")),
            Stream::Stdout => None,
        };
        let segments = self.ansi.feed(text);
        let delta = self.console.append(&segments, style.as_ref());
        self.delta.then(delta);
        if let (Some(test), Some(tree)) = (self.current_test, &mut self.tests) {
            if stream != Stream::System && attribute {
                tree.add_output(test, text);
            }
        }
    }

    /// Apply a report (JUnit XML, Jest JSON, a report log) to the tree; `false` when it is
    /// not one.
    fn apply_report(&mut self, xml: &str) -> bool {
        if self.tests.is_none() {
            return false;
        }
        match document_events(xml, self.flavor) {
            Ok(events) => {
                self.apply_events(&events);
                true
            }
            Err(e) => {
                self.write(
                    &format!("Cannot read the test report: {e}\n"),
                    Stream::Stderr,
                );
                false
            }
        }
    }

    /// Apply events to the tree and follow the running test (its output is attributed to it).
    fn apply_events(&mut self, events: &[TestEvent]) {
        let Some(tree) = self.tests.as_mut() else {
            return;
        };
        for e in events {
            self.tests_changed |= tree.apply_event(e);
            match e {
                TestEvent::TestStarted { .. } => self.current_test = tree.running_test(),
                TestEvent::TestFinished { .. } => self.current_test = None,
                _ => {}
            }
        }
    }

    /// The test tree (for the test history), `None` for runs without tests.
    pub fn tests(&self) -> Option<&TestTree> {
        self.tests.as_ref()
    }

    /// Hand complete stderr lines to the line parser (cargo names the test binary there).
    fn observe_stderr(&mut self, text: &str) {
        let Some(parser) = self.parser.as_mut() else {
            return;
        };
        let all = std::mem::take(&mut self.stderr_partial) + text;
        let mut parts: Vec<&str> = all.split('\n').collect();
        self.stderr_partial = parts.pop().unwrap_or_default().to_string();
        for line in parts {
            parser.stderr(line.trim_end_matches('\r'));
        }
    }

    /// Test output of a streaming format: protocol lines go to the tree, the rest to the
    /// console.
    fn write_test_output(&mut self, text: &str, flush_all: bool) {
        let all = std::mem::take(&mut self.partial) + text;
        let mut parts: Vec<&str> = all.split('\n').collect();
        if flush_all {
            if parts.last() == Some(&"") {
                parts.pop();
            }
        } else {
            self.partial = parts.pop().unwrap_or_default().to_string();
        }
        let mut console_text = String::new();
        for line in parts {
            let parsed = match self.parser.as_mut() {
                Some(p) => p.line(line),
                None => return,
            };
            if parsed.events.is_empty() && !parsed.attributed {
                if let Some(c) = parsed.console {
                    console_text.push_str(&c);
                }
                continue;
            }
            if !console_text.is_empty() {
                self.write(&std::mem::take(&mut console_text), Stream::Stdout);
            }
            self.apply_events(&parsed.events);
            if let Some(c) = parsed.console {
                if parsed.attributed {
                    self.write_attributed(&c, Stream::Stdout, false);
                } else {
                    console_text.push_str(&c);
                }
            }
        }
        if !console_text.is_empty() {
            self.write(&console_text, Stream::Stdout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::ConsoleLine;
    use crate::teamcity::TestStatus;

    fn run(output: &mut RunOutput, inputs: &[RunInput]) -> (Vec<ConsoleLine>, Vec<OutputEvent>) {
        let mut lines = Vec::new();
        let mut all = Vec::new();
        for i in inputs {
            for e in output.handle(i) {
                if let OutputEvent::Console(d) = &e {
                    d.apply_to(&mut lines);
                }
                all.push(e);
            }
        }
        (lines, all)
    }

    fn out(text: &str) -> RunInput {
        RunInput::Output {
            stream: "stdout".into(),
            text: text.into(),
        }
    }

    #[test]
    fn plain_runs_show_command_output_and_exit() {
        let mut o = RunOutput::new(&RunOutputOptions::default());
        let (lines, _) = run(
            &mut o,
            &[
                RunInput::Started {
                    command_line: "php a.php".into(),
                },
                out("hello\x1b[32m green"),
                out("\x1b[0m\n"),
                RunInput::Exited {
                    code: None,
                    signal: Some(15),
                },
            ],
        );
        let texts: Vec<String> = lines.iter().map(ConsoleLine::text).collect();
        assert_eq!(
            texts,
            [
                "php a.php",
                "hello green",
                "",
                "Process finished with exit code 143 (interrupted by signal 15: SIGTERM)",
                ""
            ]
        );
        assert_eq!(
            lines[0].segments[0].style.fg.as_deref(),
            Some("var(--fg-muted)")
        );
        assert_eq!(
            lines[1].segments[1].style.fg.as_deref(),
            Some("var(--ansi-green)")
        );
    }

    #[test]
    fn test_runs_route_service_messages_to_the_tree() {
        let mut o = RunOutput::new(&RunOutputOptions {
            test_runner: true,
            root: Some("/p".into()),
            cwd: None,
            cycle_buffer_kb: None,
            ..Default::default()
        });
        let (lines, events) = run(
            &mut o,
            &[
                out("PHPUnit 11\n##teamcity[testSuiteStarted name='S']\n##teamcity[testStarted name='t1']\nprinted"),
                out(" by t1\n##teamcity[testFailed name='t1' message='boom' details=' /p/T.php:3|n']\n"),
                out("##teamcity[testFinished name='t1' duration='5']\n##teamcity[testSuiteFinished name='S']\nOK"),
                RunInput::Exited { code: Some(1), signal: None },
            ],
        );
        let texts: Vec<String> = lines.iter().map(ConsoleLine::text).collect();
        assert_eq!(
            texts,
            [
                "PHPUnit 11",
                "printed by t1",
                "OK",
                "",
                "Process finished with exit code 1",
                ""
            ]
        );
        let mut nodes = std::collections::BTreeMap::new();
        let mut summary = None;
        for e in events {
            if let OutputEvent::Tests {
                nodes: n,
                summary: s,
            } = e
            {
                for node in n {
                    nodes.insert(node.id, node);
                }
                summary = Some(s);
            }
        }
        let t1 = &nodes[&2];
        assert_eq!((t1.name.as_str(), t1.status), ("t1", TestStatus::Failed));
        let console: Vec<String> = t1.console.iter().map(ConsoleLine::text).collect();
        assert_eq!(console, ["printed by t1", "boom", "", "/p/T.php:3"]);
        assert_eq!(
            t1.console[3].segments[0].link.as_ref().unwrap().target,
            "T.php"
        );
        assert_eq!(summary.unwrap().text, "Tests failed: 1 of 1 test – 5 ms");
    }

    #[test]
    fn junit_on_stdout_builds_the_tree_when_the_process_exits() {
        let mut o = RunOutput::new(&RunOutputOptions {
            test_runner: true,
            test_report: TestReport::JUnitStdout,
            junit_flavor: crate::junit::JUnitFlavor::PhpSpec,
            root: Some("/p".into()),
            ..RunOutputOptions::default()
        });
        let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<testsuites tests="1"><testsuite name="Acme\Cart" tests="1">
<testcase name="it is empty" classname="spec\Acme\CartSpec" status="passed" time="0.001"/>
</testsuite></testsuites>
"#;
        let (lines, events) = run(
            &mut o,
            &[
                out("Deprecated: something\n"),
                out(&xml[..40]),
                out(&xml[40..]),
                RunInput::Exited {
                    code: Some(0),
                    signal: None,
                },
            ],
        );
        let text: Vec<String> = lines.iter().map(ConsoleLine::text).collect();
        assert_eq!(text[0], "Deprecated: something");
        assert!(!text.iter().any(|l| l.contains("testsuite")));
        let OutputEvent::Tests { summary, .. } = events.last().unwrap() else {
            panic!("no tests event");
        };
        assert_eq!(summary.text, "Tests passed: 1 of 1 test – 1 ms");
        let tree = o.tests().unwrap().snapshot();
        assert_eq!(tree[2].name, "it is empty");
        assert_eq!(tree[2].status, TestStatus::Passed);
    }

    #[test]
    fn junit_files_arrive_as_reports_before_the_exit() {
        let mut o = RunOutput::new(&RunOutputOptions {
            test_runner: true,
            test_report: TestReport::JUnitFile,
            ..RunOutputOptions::default()
        });
        let (lines, _) = run(
            &mut o,
            &[
                out("..F\n"),
                RunInput::Report {
                    xml: "<testsuite name='F'><testcase name='a'/><testcase name='b'><failure message='x'/></testcase></testsuite>".into(),
                },
                RunInput::Report { xml: "garbage".into() },
                RunInput::Exited {
                    code: Some(1),
                    signal: None,
                },
            ],
        );
        let text: Vec<String> = lines.iter().map(ConsoleLine::text).collect();
        assert_eq!(text[0], "..F");
        assert!(text[1].starts_with("Cannot read the test report"));
        let c = o.tests().unwrap().counts();
        assert_eq!((c.passed, c.failed), (1, 1));
    }

    #[test]
    fn streaming_formats_keep_their_protocol_out_of_the_console() {
        let mut o = RunOutput::new(&RunOutputOptions {
            test_runner: true,
            test_report: TestReport::Tap,
            root: Some("/p".into()),
            ..RunOutputOptions::default()
        });
        let (lines, _) = run(
            &mut o,
            &[
                out("busted 2.2\nok 1 - Cart adds"),
                out(" items\nnot ok 2 - Cart rounds\n# spec/cart_spec.lua @ 12\n"),
                out("# Failure message: boom\n1..2\n"),
                RunInput::Exited {
                    code: Some(1),
                    signal: None,
                },
            ],
        );
        let texts: Vec<String> = lines.iter().map(ConsoleLine::text).collect();
        assert_eq!(texts[0], "busted 2.2");
        assert!(!texts
            .iter()
            .any(|t| t.starts_with("ok ") || t.starts_with('#')));
        let nodes = o.tests().unwrap().snapshot();
        assert_eq!(nodes[1].name, "Cart adds items");
        assert_eq!(nodes[2].status, TestStatus::Failed);
        assert_eq!(nodes[2].message.as_deref(), Some("boom"));

        // Go: test output shows in the console and in the test's own console.
        let mut go = RunOutput::new(&RunOutputOptions {
            test_runner: true,
            test_report: TestReport::GoTestJson,
            ..RunOutputOptions::default()
        });
        let (lines, _) = run(
            &mut go,
            &[
                out("{\"Action\":\"run\",\"Package\":\"p\",\"Test\":\"TestA\"}\n"),
                out("{\"Action\":\"output\",\"Package\":\"p\",\"Test\":\"TestA\",\"Output\":\"hello\\n\"}\n"),
                out("{\"Action\":\"pass\",\"Package\":\"p\",\"Test\":\"TestA\",\"Elapsed\":0.5}\n"),
                RunInput::Exited {
                    code: Some(0),
                    signal: None,
                },
            ],
        );
        assert_eq!(lines[0].text(), "hello");
        let nodes = go.tests().unwrap().snapshot();
        let console: Vec<String> = nodes[2].console.iter().map(ConsoleLine::text).collect();
        assert_eq!(console, ["hello"]);
        assert_eq!(nodes[2].duration, Some(500));
    }

    #[test]
    fn jest_json_on_stdout_is_read_at_the_exit() {
        let mut o = RunOutput::new(&RunOutputOptions {
            test_runner: true,
            test_report: TestReport::JestJson,
            ..RunOutputOptions::default()
        });
        let json = r#"{"numTotalTests":1,"testResults":[{"name":"/p/a.test.js","status":"passed","assertionResults":[{"ancestorTitles":[],"title":"works","status":"passed","duration":2}]}]}"#;
        let (lines, _) = run(
            &mut o,
            &[
                out("Determining test suites to run...\n"),
                out(json),
                RunInput::Exited {
                    code: Some(0),
                    signal: None,
                },
            ],
        );
        assert_eq!(lines[0].text(), "Determining test suites to run...");
        assert_eq!(o.tests().unwrap().counts().passed, 1);
    }

    #[test]
    fn drops_the_oldest_lines_beyond_the_cycle_buffer() {
        let mut o = RunOutput::new(&RunOutputOptions {
            cycle_buffer_kb: Some(1),
            ..RunOutputOptions::default()
        });
        let line = format!("{}\n", "x".repeat(99));
        let inputs: Vec<RunInput> = (0..30)
            .map(|_| RunInput::Output {
                stream: "stdout".into(),
                text: line.clone(),
            })
            .collect();
        let (lines, events) = run(&mut o, &inputs);
        // 1 KB holds ten 100-byte lines (plus the open line after the last break).
        assert!(lines.len() <= 11, "{}", lines.len());
        assert!(lines.len() >= 9, "{}", lines.len());
        assert!(events
            .iter()
            .any(|e| matches!(e, OutputEvent::Console(d) if d.drop_front > 0)));
        // Without a limit everything stays.
        let mut all = RunOutput::new(&RunOutputOptions::default());
        assert_eq!(run(&mut all, &inputs).0.len(), 31);
    }
}
