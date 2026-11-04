//! Commands the agent runs in its sandbox: parsing a command line without a shell, the
//! allowlist that skips the approval, the programs that are always refused (network
//! clients), running a process with a timeout and evaluating a test run.

use std::collections::BTreeMap;
use std::path::PathBuf;
#[cfg(feature = "agent")]
use std::time::Duration;

use forge_output::run::TestReport;
use forge_output::testevents::TestEvent;
use serde::{Deserialize, Serialize};

use crate::task::TestResult;

/// Programs the agent may never run: it gets no network tools.
const NETWORK_PROGRAMS: [&str; 12] = [
    "curl", "wget", "ssh", "scp", "sftp", "ftp", "telnet", "nc", "ncat", "netcat", "rsync", "socat",
];

/// Most characters of output handed back to the agent.
pub const OUTPUT_LIMIT: usize = 12_000;

/// A program to run, without a shell.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl ShellCommand {
    /// The command line as shown in the log.
    pub fn display(&self) -> String {
        std::iter::once(&self.program)
            .chain(&self.args)
            .map(|a| quote(a))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn quote(a: &str) -> String {
    if !a.is_empty() && !a.contains([' ', '"', '\'', '\t']) {
        a.to_string()
    } else {
        format!("\"{}\"", a.replace('"', "\\\""))
    }
}

/// What a process did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOutput {
    pub exit_code: Option<i32>,
    /// stdout followed by stderr.
    pub output: String,
    pub timed_out: bool,
}

/// A test command plus the format its results arrive in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestSpec {
    /// Shown in the log, e.g. the run configuration's name.
    pub label: String,
    pub command: ShellCommand,
    /// Line format on stdout; `None`: only the exit code counts.
    pub report: Option<TestReport>,
}

/// Split a command line into words: whitespace separates, single and double quotes group,
/// a backslash escapes inside double quotes and outside quotes. Shell operators are refused
/// because no shell runs the command.
pub fn split_command(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return Err("unterminated ' quote".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(ch) => cur.push(ch),
                            None => return Err("unterminated \" quote".into()),
                        },
                        Some(ch) => cur.push(ch),
                        None => return Err("unterminated \" quote".into()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(ch) = chars.next() {
                    cur.push(ch);
                }
            }
            '|' | '&' | ';' | '<' | '>' | '`' => {
                return Err(format!(
                    "shell operator {c:?} is not supported: commands run without a shell, one program at a time"
                ))
            }
            '$' if chars.peek() == Some(&'(') => {
                return Err("command substitution is not supported".into())
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    if words.is_empty() {
        return Err("the command is empty".into());
    }
    Ok(words)
}

fn program_name(program: &str) -> String {
    let base = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let base = base.to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).to_string()
}

/// Why `words` may never run (network clients), if so.
pub fn refused(words: &[String]) -> Option<String> {
    let program = program_name(words.first()?);
    NETWORK_PROGRAMS
        .contains(&program.as_str())
        .then(|| format!("{program} is not available to agents (no network tools)"))
}

/// Whether `words` starts with one of the allowlisted command prefixes
/// (`ai.agentTasks.allowedCommands`, e.g. `composer test`, `npm run lint`).
pub fn allowlisted(words: &[String], allowlist: &[String]) -> bool {
    allowlist.iter().any(|entry| {
        let Ok(prefix) = split_command(entry) else {
            return false;
        };
        prefix.len() <= words.len()
            && prefix.iter().zip(words).enumerate().all(|(i, (p, w))| {
                if i == 0 {
                    program_name(p) == program_name(w)
                } else {
                    p == w
                }
            })
    })
}

/// Keep the end of long output (where test summaries and errors are).
pub fn tail(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_string();
    }
    let rest: String = text.chars().skip(count - limit).collect();
    format!("… ({} characters cut)\n{rest}", count - limit)
}

/// Run `cmd`, capturing its output, killed after `timeout` or when `cancel` fires.
#[cfg(feature = "agent")]
pub async fn run_process(
    cmd: &ShellCommand,
    timeout: Duration,
    cancel: &forge_ai::CancellationToken,
) -> RunOutput {
    use tokio::process::Command;
    let mut c = Command::new(&cmd.program);
    c.args(&cmd.args)
        .current_dir(&cmd.cwd)
        .envs(&cmd.env)
        .env("CI", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let child = match c.spawn() {
        Ok(child) => child,
        Err(e) => {
            return RunOutput {
                exit_code: None,
                output: format!("could not start {}: {e}", cmd.program),
                timed_out: false,
            }
        }
    };
    let wait = child.wait_with_output();
    tokio::select! {
        r = tokio::time::timeout(timeout, wait) => match r {
            Ok(Ok(out)) => {
                let mut output = String::from_utf8_lossy(&out.stdout).into_owned();
                let err = String::from_utf8_lossy(&out.stderr);
                if !err.trim().is_empty() {
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    output.push_str(&err);
                }
                RunOutput { exit_code: out.status.code(), output, timed_out: false }
            }
            Ok(Err(e)) => RunOutput { exit_code: None, output: format!("{}: {e}", cmd.program), timed_out: false },
            Err(_) => RunOutput {
                exit_code: None,
                output: format!("{} timed out after {}s and was stopped", cmd.program, timeout.as_secs()),
                timed_out: true,
            },
        },
        _ = cancel.cancelled() => RunOutput { exit_code: None, output: "cancelled".into(), timed_out: false },
    }
}

/// Evaluate a test run: parse its results when the format is known, else go by the exit
/// code. Green means exit code 0 and no failed test.
pub fn evaluate_tests(run: &RunOutput, report: Option<TestReport>) -> TestResult {
    let mut passed = 0;
    let mut failed = 0;
    let mut failures = Vec::new();
    let mut failed_names = std::collections::HashSet::new();
    if let Some(mut parser) = report.and_then(forge_output::formats::line_parser) {
        let mut events = Vec::new();
        for line in run.output.lines() {
            events.extend(parser.line(line).events);
        }
        events.extend(parser.finish());
        for e in events {
            match e {
                TestEvent::TestFailed { name, message, .. } => {
                    if failed_names.insert(name.clone()) {
                        failed += 1;
                        if failures.len() < 10 {
                            let msg = message
                                .lines()
                                .next()
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            failures.push(if msg.is_empty() {
                                name
                            } else {
                                format!("{name}: {msg}")
                            });
                        }
                    }
                }
                TestEvent::TestFinished { name, .. } if !failed_names.contains(&name) => {
                    passed += 1
                }
                _ => {}
            }
        }
    }
    let green = run.exit_code == Some(0) && failed == 0 && !run.timed_out;
    TestResult {
        green,
        exit_code: run.exit_code,
        passed,
        failed,
        failures,
        output_tail: tail(&run.output, 4000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        split_command(s).unwrap()
    }

    #[test]
    fn splits_like_a_shell_without_running_one() {
        assert_eq!(words("php artisan test"), ["php", "artisan", "test"]);
        assert_eq!(
            words(r#"vendor/bin/phpunit --filter 'Order test' "a \"b\"" x\ y"#),
            [
                "vendor/bin/phpunit",
                "--filter",
                "Order test",
                "a \"b\"",
                "x y"
            ]
        );
        assert_eq!(words("echo ''"), ["echo", ""]);
        assert!(split_command("ls | grep a").is_err());
        assert!(split_command("make && rm -rf /").is_err());
        assert!(split_command("echo $(id)").is_err());
        assert!(split_command("echo 'open").is_err());
        assert!(split_command("   ").is_err());
        assert_eq!(words("echo '|'"), ["echo", "|"]);
    }

    #[test]
    fn allowlist_matches_word_prefixes() {
        let allow = vec![
            "composer test".to_string(),
            "npm run lint".to_string(),
            "phpunit".into(),
        ];
        assert!(allowlisted(&words("composer test --filter x"), &allow));
        assert!(!allowlisted(&words("composer testing"), &allow));
        assert!(!allowlisted(&words("composer require evil/pkg"), &allow));
        assert!(allowlisted(&words("/usr/bin/phpunit"), &allow));
        assert!(!allowlisted(&words("npm run"), &allow));
        assert!(refused(&words("curl https://example.com")).is_some());
        assert!(refused(&words("/usr/bin/WGET.exe x")).is_some());
        assert!(refused(&words("php -v")).is_none());
        assert_eq!(
            ShellCommand {
                program: "php".into(),
                args: vec!["a b".into()],
                ..Default::default()
            }
            .display(),
            "php \"a b\""
        );
    }

    #[test]
    fn evaluates_test_runs() {
        let tc = "##teamcity[testStarted name='testA']\n##teamcity[testFinished name='testA']\n##teamcity[testStarted name='testB']\n##teamcity[testFailed name='testB' message='Failed asserting that 2 is 3']\n##teamcity[testFinished name='testB']\n";
        let r = evaluate_tests(
            &RunOutput {
                exit_code: Some(1),
                output: tc.into(),
                timed_out: false,
            },
            Some(TestReport::TeamCity),
        );
        assert!(!r.green);
        assert_eq!((r.passed, r.failed), (1, 1));
        assert_eq!(r.failures, ["testB: Failed asserting that 2 is 3"]);
        let ok = evaluate_tests(
            &RunOutput {
                exit_code: Some(0),
                output: "OK (3 tests)".into(),
                timed_out: false,
            },
            None,
        );
        assert!(ok.green);
        assert_eq!(tail("abcdef", 3), "… (3 characters cut)\ndef");
    }

    #[cfg(feature = "agent")]
    #[tokio::test]
    async fn runs_processes_with_a_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = forge_ai::CancellationToken::new();
        #[cfg(unix)]
        {
            let cmd = ShellCommand {
                program: "sh".into(),
                args: vec!["-c".into(), "echo out; echo err >&2; exit 3".into()],
                cwd: dir.path().into(),
                env: BTreeMap::new(),
            };
            let r = run_process(&cmd, Duration::from_secs(10), &cancel).await;
            assert_eq!(r.exit_code, Some(3));
            assert_eq!(r.output, "out\nerr\n");
            let slow = ShellCommand {
                program: "sleep".into(),
                args: vec!["5".into()],
                cwd: dir.path().into(),
                env: BTreeMap::new(),
            };
            let r = run_process(&slow, Duration::from_millis(100), &cancel).await;
            assert!(r.timed_out);
        }
        let missing = ShellCommand {
            program: "forge-no-such-program".into(),
            cwd: dir.path().into(),
            ..Default::default()
        };
        let r = run_process(&missing, Duration::from_secs(1), &cancel).await;
        assert!(r.output.starts_with("could not start"));
    }
}
