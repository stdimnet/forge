//! The output of one run: console lines for every stream, and for test runners the test
//! tree built from TeamCity service messages on stdout (those lines are not shown in the
//! console). Output while a test runs is attributed to that test.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::ansi::{AnsiParser, Style};
use crate::console::{ConsoleDelta, ConsoleModel};
use crate::junit::{junit_messages, JUnitFlavor};
use crate::links::{LinkContext, OutputPathMapping};
use crate::teamcity::{parse_service_message, TestNode, TestSummary, TestTree};

/// Where a test run reports its results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TestReport {
    /// TeamCity service messages on stdout (PHPUnit `--teamcity`, Pest, Jest).
    #[default]
    TeamCity,
    /// A JUnit XML document on stdout (PHPSpec `--format=junit`): stdout is collected and
    /// turned into the tree when the process exits.
    JUnitStdout,
    /// JUnit XML files the runner writes (Codeception `--xml`, Behat `--format=junit`): the
    /// console shows stdout, the backend hands the files over as [`RunInput::Report`].
    JUnitFile,
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
    /// Stdout of a [`TestReport::JUnitStdout`] run, parsed when it exits.
    junit: String,
    /// Incomplete stdout line of a test run (service messages are line based).
    partial: String,
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
            tests: opts
                .test_runner
                .then(|| TestTree::new("Test Results", links)),
            report: opts.test_report,
            flavor: opts.junit_flavor,
            junit: String::new(),
            partial: String::new(),
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
                    && self.report == TestReport::JUnitStdout
                {
                    self.junit.push_str(text);
                } else if self.tests.is_some()
                    && stream == Stream::Stdout
                    && self.report == TestReport::TeamCity
                {
                    self.write_test_output(text, false);
                } else {
                    self.write(text, stream);
                }
            }
            RunInput::Input { text } => self.write(text, Stream::Stdout),
            RunInput::Report { xml } => {
                self.apply_report(xml);
            }
            RunInput::Exited { code, signal } => {
                if self.tests.is_some() && !self.partial.is_empty() {
                    self.write_test_output("", true);
                }
                if self.tests.is_some() && !self.junit.is_empty() {
                    let all = std::mem::take(&mut self.junit);
                    // Anything before the document (warnings, deprecations) is console text.
                    let start = all
                        .find("<?xml")
                        .or_else(|| all.find("<testsuites"))
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
        let style = match stream {
            Stream::Stderr => Some(Style::fg("var(--danger)")),
            Stream::System => Some(Style::fg("var(--fg-muted)")),
            Stream::Stdout => None,
        };
        let segments = self.ansi.feed(text);
        let delta = self.console.append(&segments, style.as_ref());
        self.delta.then(delta);
        if let (Some(test), Some(tree)) = (self.current_test, &mut self.tests) {
            if stream != Stream::System {
                tree.add_output(test, text);
            }
        }
    }

    /// Apply a JUnit report to the tree; `false` when it is not one.
    fn apply_report(&mut self, xml: &str) -> bool {
        let Some(tree) = self.tests.as_mut() else {
            return false;
        };
        match junit_messages(xml, self.flavor) {
            Ok(messages) => {
                for m in &messages {
                    self.tests_changed |= tree.apply(m);
                }
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

    /// The test tree (for the test history), `None` for runs without tests.
    pub fn tests(&self) -> Option<&TestTree> {
        self.tests.as_ref()
    }

    /// Test output: `##teamcity[...]` lines go to the tree, the rest to the console.
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
            let Some(msg) = parse_service_message(line) else {
                console_text.push_str(line);
                console_text.push('\n');
                continue;
            };
            if !console_text.is_empty() {
                self.write(&std::mem::take(&mut console_text), Stream::Stdout);
            }
            let tree = self.tests.as_mut().expect("test run");
            self.tests_changed |= tree.apply(&msg);
            match msg.name.as_str() {
                "testStarted" => self.current_test = tree.running_test(),
                "testFinished" => self.current_test = None,
                _ => {}
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
