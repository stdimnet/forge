//! tox runs (`tox -e py312,lint`): every environment is a test of the tree. tox 4 and tox 3
//! print what an environment does with its name in front, so the parser attributes those
//! lines to the environment and reads its result:
//!
//! - tox 4: `py312: install_deps> python -I -m pip install pytest`,
//!   `py312: commands[0]> pytest`, then `py312: OK ✔ in 3.1 seconds`,
//!   `py312: FAIL ✖ in 1.2 seconds` (after `py312: exit 1 (0.52 seconds) …`) or
//!   `py39: SKIP ⚠ in 0.01 seconds`; the summary repeats them indented
//!   (`  py312: OK (3.10=setup[2.20]+cmd[0.90] seconds)`).
//! - tox 3: `py312 run-test: commands[0] | pytest`, `py312 installdeps: pytest`, then the
//!   summary `  py312: commands succeeded`, `ERROR:   py312: commands failed` or
//!   `  py39: SKIP: InterpreterNotFound: python3.9`.
//!
//! The parser keeps every line for the console; the environments' lines are also their
//! tests' output.

use crate::testevents::{LineParser, ParsedLine, TestEvent};

#[derive(Debug, Default)]
pub struct ToxParser {
    /// The environment whose lines arrive now.
    current: Option<String>,
    /// Environments that started (their test is open or done).
    started: Vec<String>,
    /// Environments with a result.
    done: Vec<String>,
    /// The last `exit N` line of an environment: the failure message.
    exit: Option<(String, String)>,
}

impl ToxParser {
    pub fn new() -> Self {
        Self::default()
    }

    fn start(&mut self, env: &str, out: &mut Vec<TestEvent>) {
        if self.current.as_deref() == Some(env) {
            return;
        }
        if !self.started.iter().any(|e| e == env) {
            self.started.push(env.to_string());
            out.push(TestEvent::TestStarted {
                name: env.to_string(),
                location: None,
            });
        }
        self.current = Some(env.to_string());
    }

    fn finish(&mut self, env: &str, result: EnvResult, detail: &str, out: &mut Vec<TestEvent>) {
        if self.done.iter().any(|e| e == env) {
            return;
        }
        if result == EnvResult::Skip {
            out.push(TestEvent::TestIgnored {
                name: env.to_string(),
                message: Some(detail.to_string()).filter(|d| !d.is_empty()),
                location: None,
            });
            if self.started.iter().any(|e| e == env) {
                // tox 4 announces skipped environments with a line of their own first.
                out.push(TestEvent::TestFinished {
                    name: env.to_string(),
                    duration: None,
                });
            }
            self.done.push(env.to_string());
            if self.current.as_deref() == Some(env) {
                self.current = None;
            }
            return;
        }
        self.start(env, out);
        if result == EnvResult::Fail {
            let message = match &self.exit {
                Some((e, m)) if e == env => m.clone(),
                _ => format!("{env}: {}", detail.trim()),
            };
            out.push(TestEvent::TestFailed {
                name: env.to_string(),
                message,
                details: None,
                expected: None,
                actual: None,
                location: None,
            });
        }
        out.push(TestEvent::TestFinished {
            name: env.to_string(),
            duration: seconds(detail),
        });
        self.done.push(env.to_string());
        self.current = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvResult {
    Ok,
    Fail,
    Skip,
}

/// An environment name as tox prints it (`py312`, `py312-django50`, `lint`, `docs.fast`).
fn is_env_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
        && s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
}

/// `in 3.1 seconds`, `(3.10=setup[2.20]+cmd[0.90] seconds)`, `(0.52 seconds)` → ms.
fn seconds(text: &str) -> Option<u64> {
    if !text.contains(" seconds") {
        return None;
    }
    let after = match (text.find('('), text.find("in ")) {
        (Some(p), _) => &text[p + 1..],
        (None, Some(p)) => &text[p + 3..],
        _ => return None,
    };
    let number: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = number.parse().ok()?;
    Some((value * 1000.0).round() as u64)
}

/// A tox 4 result line: `py312: OK ✔ in 3.1 seconds`, `  py312: FAIL code 1 (…)`.
fn result_line(line: &str) -> Option<(String, EnvResult, String)> {
    let t = line.trim_start_matches("ERROR:").trim();
    let (env, rest) = t.split_once(": ")?;
    if !is_env_name(env) {
        return None;
    }
    let word: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let result = match word.as_str() {
        "OK" => EnvResult::Ok,
        "FAIL" => EnvResult::Fail,
        "SKIP" => EnvResult::Skip,
        "commands" if rest.starts_with("commands succeeded") => EnvResult::Ok,
        "commands" if rest.starts_with("commands failed") => EnvResult::Fail,
        _ => return None,
    };
    let after = &rest[word.len()..];
    let tail_ok = after.is_empty()
        || [" ✔", " ✖", " ⚠", " (", " in ", " code", ":", " succeeded", " failed"]
            .iter()
            .any(|p| after.starts_with(p));
    if !tail_ok {
        return None;
    }
    let detail = after
        .trim_start_matches(':')
        .trim()
        .to_string();
    let detail = if result == EnvResult::Fail && word == "commands" {
        rest.to_string()
    } else {
        detail
    };
    Some((env.to_string(), result, detail))
}

/// The environment of an action line: `py312: commands[0]> pytest` (tox 4) or
/// `py312 run-test: commands[0] | pytest` (tox 3).
fn action_env(line: &str) -> Option<String> {
    if line.starts_with(' ') {
        return None;
    }
    if let Some((env, rest)) = line.split_once(": ") {
        if is_env_name(env) {
            let action: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '>')
                .collect();
            if rest[action.len()..].starts_with('>')
                || rest.starts_with("exit ")
                || rest.starts_with("skipped because")
                || rest.starts_with("recreate env")
                || rest.starts_with("venv>")
            {
                return Some(env.to_string());
            }
        }
    }
    let (head, _) = line.split_once(": ")?;
    let (env, action) = head.split_once(' ')?;
    (is_env_name(env)
        && matches!(
            action,
            "create" | "recreate" | "installdeps" | "inst" | "installed" | "run-test-pre"
                | "run-test" | "develop-inst" | "inst-nodeps" | "commands_pre"
        ))
    .then(|| env.to_string())
}

impl LineParser for ToxParser {
    fn line(&mut self, line: &str) -> ParsedLine {
        let clean = super::strip_ansi(line);
        let mut events = Vec::new();
        if let Some((env, result, detail)) = result_line(&clean) {
            self.finish(&env, result, &detail, &mut events);
            return ParsedLine {
                events,
                console: Some(format!("{line}\n")),
                attributed: false,
            };
        }
        if let Some(env) = action_env(&clean) {
            if !self.done.iter().any(|e| *e == env) {
                self.start(&env, &mut events);
            }
            if let Some(rest) = clean
                .strip_prefix(&format!("{env}: "))
                .filter(|r| r.starts_with("exit "))
            {
                self.exit = Some((env.clone(), format!("{env}: {rest}")));
            }
        }
        let attributed = match &self.current {
            Some(env) => {
                events.push(TestEvent::TestOutput {
                    name: env.clone(),
                    text: format!("{clean}\n"),
                    stderr: false,
                });
                true
            }
            None => false,
        };
        ParsedLine {
            events,
            console: Some(format!("{line}\n")),
            attributed,
        }
    }

    fn finish(&mut self) -> Vec<TestEvent> {
        // A run that stopped in the middle of an environment.
        let mut out = Vec::new();
        if let Some(env) = self.current.take() {
            if !self.done.contains(&env) {
                out.push(TestEvent::TestFinished {
                    name: env,
                    duration: None,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::tests::{fixture, run_lines, tree_of};
    use crate::teamcity::TestStatus;

    #[test]
    fn tox4_environments_become_tests() {
        let mut p = ToxParser::new();
        let events = run_lines(&mut p, &fixture("tox4.txt"));
        let tree = tree_of(&events);
        let counts = tree.counts();
        assert_eq!(counts.total, 3, "{events:#?}");
        assert_eq!(counts.passed, 1);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.ignored, 1);
        let nodes = tree.snapshot();
        let py312 = nodes.iter().find(|n| n.name == "py312").unwrap();
        assert_eq!(py312.status, TestStatus::Passed);
        assert_eq!(py312.duration, Some(3100));
        let lint = nodes.iter().find(|n| n.name == "lint").unwrap();
        assert_eq!(lint.status, TestStatus::Failed);
        assert!(lint.message.as_deref().unwrap().contains("exit 1"), "{lint:?}");
        assert!(events.iter().any(|e| matches!(e,
            TestEvent::TestOutput { name, text, .. } if name == "lint" && text.contains("ruff check"))));
    }

    #[test]
    fn tox3_summaries_give_the_results() {
        let text = "py312 create: /p/.tox/py312\n\
                    py312 installdeps: pytest\n\
                    py312 run-test: commands[0] | pytest\n\
                    ===== 3 passed in 0.02s =====\n\
                    lint run-test: commands[0] | ruff check .\n\
                    src/a.py:1:1: F401 unused import\n\
                    ERROR: InvocationError for command /p/.tox/lint/bin/ruff check . (exited with code 1)\n\
                    ___ summary ___\n\
                    \x20 py312: commands succeeded\n\
                    ERROR:   lint: commands failed\n\
                    \x20 py39: SKIP: InterpreterNotFound: python3.9\n";
        let mut p = ToxParser::new();
        let events = run_lines(&mut p, text);
        let tree = tree_of(&events);
        assert_eq!(tree.counts().passed, 1);
        assert_eq!(tree.counts().failed, 1);
        assert_eq!(tree.counts().ignored, 1);
        let nodes = tree.snapshot();
        let py39 = nodes.iter().find(|n| n.name == "py39").unwrap();
        assert_eq!(
            py39.message.as_deref(),
            Some("InterpreterNotFound: python3.9")
        );
    }

    #[test]
    fn helpers_read_names_and_durations() {
        assert_eq!(seconds("OK ✔ in 3.1 seconds"), Some(3100));
        assert_eq!(seconds("OK (3.10=setup[2.20]+cmd[0.90] seconds)"), Some(3100));
        assert_eq!(seconds("OK"), None);
        assert!(is_env_name("py312-django50") && !is_env_name(""));
        assert!(result_line("note: OK then").is_none());
        assert_eq!(action_env("py312: commands[0]> pytest").as_deref(), Some("py312"));
        assert_eq!(action_env("  congratulations :) (5.12 seconds)"), None);
        assert_eq!(action_env("ImportError: no module"), None);
    }
}
