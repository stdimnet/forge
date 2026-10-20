//! The output of one run: console lines for every stream, and for test runners the test
//! tree built from TeamCity service messages on stdout (those lines are not shown in the
//! console). Output while a test runs is attributed to that test.

use serde::{Deserialize, Serialize};

use crate::ansi::{AnsiParser, Style};
use crate::console::{ConsoleDelta, ConsoleModel};
use crate::links::LinkContext;
use crate::teamcity::{parse_service_message, TestNode, TestSummary, TestTree};

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
    /// Incomplete stdout line of a test run (service messages are line based).
    partial: String,
    current_test: Option<usize>,
    delta: ConsoleDelta,
    tests_changed: bool,
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
        let links = LinkContext::new(opts.root.as_deref(), opts.cwd.as_deref());
        RunOutput {
            ansi: AnsiParser::new(),
            console: ConsoleModel::new(links.clone()),
            tests: opts
                .test_runner
                .then(|| TestTree::new("Test Results", links)),
            partial: String::new(),
            current_test: None,
            delta: ConsoleDelta::default(),
            tests_changed: false,
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
                if self.tests.is_some() && stream == Stream::Stdout {
                    self.write_test_output(text, false);
                } else {
                    self.write(text, stream);
                }
            }
            RunInput::Input { text } => self.write(text, Stream::Stdout),
            RunInput::Exited { code, signal } => {
                if self.tests.is_some() && !self.partial.is_empty() {
                    self.write_test_output("", true);
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
        let delta = std::mem::take(&mut self.delta);
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
}
