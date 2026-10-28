//! The run loops: Forge's built-in tool-calling agent over a `forge-ai` provider
//! ([`run_builtin`]) and the run-test-fix loop around an agent CLI ([`run_cli`]). Both stop
//! when the work is done and the tests pass, or when the budget is used up.

use std::time::Duration;

use async_trait::async_trait;
use forge_ai::{CancellationToken, CompletionRequest, Message, Provider};

use crate::commands::{
    allowlisted, evaluate_tests, refused, split_command, tail, RunOutput, ShellCommand, TestSpec,
    OUTPUT_LIMIT,
};
use crate::task::{ApprovalState, Outcome, OutcomeKind, StepKind, TestResult};
use crate::tools::{parse_reply, system_prompt, ToolCall};
use crate::workspace::Workspace;

/// Limits of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    /// Model turns (built-in agent) or CLI turns.
    pub max_steps: u32,
    /// Test runs, including the automatic ones after `finish`.
    pub max_test_runs: u32,
    pub command_timeout: Duration,
    pub test_timeout: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_steps: 40,
            max_test_runs: 6,
            command_timeout: Duration::from_secs(120),
            test_timeout: Duration::from_secs(600),
        }
    }
}

/// Everything a run needs to know about its task.
pub struct TaskContext {
    pub prompt: String,
    pub workspace: Workspace,
    /// `ai.agentTasks.allowedCommands`.
    pub allowlist: Vec<String>,
    pub tests: Option<TestSpec>,
    pub budget: Budget,
}

/// What a run needs from its surroundings: processes, the user's approval and the log.
#[async_trait]
pub trait Host: Send + Sync {
    async fn run(&self, cmd: &ShellCommand, timeout: Duration) -> RunOutput;
    /// Ask the user whether `command` may run (the log shows an approval step at `step`).
    async fn approve(&self, step: usize, command: &str) -> bool;
    /// Append a step to the log; returns its index.
    fn log(&self, kind: StepKind) -> usize;
    /// Replace a step of the log.
    fn update(&self, index: usize, kind: StepKind);
}

/// Run the tests once and log the result.
pub async fn run_tests(spec: &TestSpec, budget: &Budget, host: &dyn Host) -> TestResult {
    host.log(StepKind::Info {
        text: format!("Running {}: {}", spec.label, spec.command.display()),
    });
    let out = host.run(&spec.command, budget.test_timeout).await;
    let result = evaluate_tests(&out, spec.report);
    host.log(StepKind::Tests {
        result: result.clone(),
    });
    result
}

/// What the agent is told about a failing test run.
pub fn failure_report(r: &TestResult) -> String {
    let mut s = String::new();
    if r.failed > 0 {
        s.push_str(&format!("{} passed, {} failed.\n", r.passed, r.failed));
        for f in &r.failures {
            s.push_str(&format!("- {f}\n"));
        }
    } else {
        s.push_str(&format!(
            "The test command failed (exit code {}).\n",
            r.exit_code.map_or("none".into(), |c| c.to_string())
        ));
    }
    s.push_str("\nOutput (end):\n");
    s.push_str(&r.output_tail);
    s
}

/// Run a command the agent asked for: refused programs never run, allowlisted ones run
/// right away, all others after the user's approval.
pub async fn run_command(
    ctx: &TaskContext,
    host: &dyn Host,
    command: &str,
) -> Result<String, String> {
    let words = split_command(command)?;
    if let Some(why) = refused(&words) {
        return Err(why);
    }
    if !allowlisted(&words, &ctx.allowlist) {
        let step = host.log(StepKind::Approval {
            command: command.to_string(),
            state: ApprovalState::Pending,
        });
        let allowed = host.approve(step, command).await;
        host.update(
            step,
            StepKind::Approval {
                command: command.to_string(),
                state: if allowed {
                    ApprovalState::Allowed
                } else {
                    ApprovalState::Denied
                },
            },
        );
        if !allowed {
            return Err("The user denied this command. Do not ask for it again.".into());
        }
    }
    let cmd = ShellCommand {
        program: words[0].clone(),
        args: words[1..].to_vec(),
        cwd: ctx.workspace.root().to_path_buf(),
        env: Default::default(),
    };
    let out = host.run(&cmd, ctx.budget.command_timeout).await;
    let text = tail(&out.output, OUTPUT_LIMIT);
    match out.exit_code {
        Some(0) => Ok(format!("exit code 0\n{text}")),
        Some(c) => Err(format!("exit code {c}\n{text}")),
        None => Err(text),
    }
}

/// Mutable state of a built-in run.
struct Run<'a> {
    ctx: &'a TaskContext,
    host: &'a dyn Host,
    test_runs: u32,
    /// Files changed since the last test run.
    dirty: bool,
    last_tests: Option<TestResult>,
}

impl Run<'_> {
    async fn tests(&mut self, spec: &TestSpec) -> TestResult {
        self.test_runs += 1;
        self.dirty = false;
        let r = run_tests(spec, &self.ctx.budget, self.host).await;
        self.last_tests = Some(r.clone());
        r
    }

    async fn execute(&mut self, call: &ToolCall) -> Result<String, String> {
        let ws = &self.ctx.workspace;
        match call {
            ToolCall::ListFiles { path } => ws.list(path),
            ToolCall::ReadFile {
                path,
                start_line,
                end_line,
            } => ws
                .read(path, *start_line, *end_line)
                .map(|t| tail(&t, OUTPUT_LIMIT * 4)),
            ToolCall::Search { query, path } => ws.search(query, path),
            ToolCall::WriteFile { path, content } => ws.write(path, content),
            ToolCall::EditFile { path, old, new } => ws.edit(path, old, new),
            ToolCall::DeleteFile { path } => ws.delete(path),
            ToolCall::RunCommand { command } => run_command(self.ctx, self.host, command).await,
            ToolCall::RunTests => {
                let Some(spec) = &self.ctx.tests else {
                    return Err("This task has no tests configured.".into());
                };
                if self.test_runs >= self.ctx.budget.max_test_runs {
                    return Err("The test run budget is used up; finish now.".into());
                }
                let r = self.tests(spec).await;
                if r.green {
                    Ok(format!("All tests pass ({} passed).", r.passed))
                } else {
                    Err(failure_report(&r))
                }
            }
            ToolCall::Finish { .. } => Ok(String::new()),
        }
    }
}

/// Forge's built-in agent: ask the model for one tool call per turn, run it in the sandbox
/// and hand back the result, until it calls `finish` with passing tests or the budget is
/// used up.
pub async fn run_builtin(
    provider: &dyn Provider,
    model: Option<String>,
    ctx: &TaskContext,
    host: &dyn Host,
    cancel: &CancellationToken,
) -> Result<Outcome, String> {
    let mut run = Run {
        ctx,
        host,
        test_runs: 0,
        dirty: false,
        last_tests: None,
    };
    let system = system_prompt(ctx.tests.is_some(), ctx.workspace.scope());
    let mut messages = vec![Message::user(format!("Task:\n{}", ctx.prompt.trim()))];
    for _ in 0..ctx.budget.max_steps {
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        let request = CompletionRequest {
            model: model.clone(),
            system: Some(system.clone()),
            messages: messages.clone(),
            max_tokens: Some(8192),
            temperature: Some(0.0),
            stop: Vec::new(),
        };
        let reply = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("cancelled".into()),
            r = provider.complete(request) => r.map_err(|e| e.to_string())?,
        };
        messages.push(Message::assistant(reply.text.clone()));
        let parsed = parse_reply(&reply.text);
        if !parsed.thought.is_empty() {
            host.log(StepKind::Thought {
                text: parsed.thought.clone(),
            });
        }
        let call = match parsed.call {
            Ok(call) => call,
            Err(e) => {
                messages.push(Message::user(e));
                continue;
            }
        };
        if let ToolCall::Finish { summary } = &call {
            let summary = Some(summary.clone()).filter(|s| !s.trim().is_empty());
            let Some(spec) = &ctx.tests else {
                return Ok(Outcome {
                    kind: OutcomeKind::Completed,
                    summary,
                    tests: None,
                });
            };
            // Verify before accepting "done".
            let result = match &run.last_tests {
                Some(r) if !run.dirty => r.clone(),
                _ if run.test_runs < ctx.budget.max_test_runs => run.tests(spec).await,
                _ => run.last_tests.clone().unwrap_or_default(),
            };
            if result.green {
                return Ok(Outcome {
                    kind: OutcomeKind::Completed,
                    summary,
                    tests: Some(result),
                });
            }
            if run.test_runs >= ctx.budget.max_test_runs {
                return Ok(Outcome {
                    kind: OutcomeKind::TestsFailing,
                    summary,
                    tests: Some(result),
                });
            }
            messages.push(Message::user(format!(
                "You are not done: the tests fail.\n{}\nFix the failures, then call finish again.",
                failure_report(&result)
            )));
            continue;
        }
        let result = run.execute(&call).await;
        if call.mutates() {
            run.dirty = true;
        }
        let (output, is_error) = match &result {
            Ok(o) => (o.clone(), false),
            Err(e) => (e.clone(), true),
        };
        host.log(StepKind::Tool {
            tool: call.name().into(),
            summary: call.summary(),
            input: call.input(),
            output: tail(&output, 4000),
            is_error,
        });
        messages.push(Message::user(format!(
            "{} {}:\n{}",
            call.name(),
            if is_error { "failed" } else { "result" },
            tail(&output, OUTPUT_LIMIT)
        )));
    }
    Ok(Outcome {
        kind: OutcomeKind::BudgetExhausted,
        summary: None,
        tests: run.last_tests,
    })
}

/// One turn of an agent CLI: send a message, wait until the CLI finished its turn.
#[async_trait]
pub trait CliTurns: Send + Sync {
    /// Returns the CLI's final text of the turn.
    async fn turn(&self, message: &str) -> Result<Option<String>, String>;
}

/// The first message to an agent CLI.
pub fn cli_prompt(ctx: &TaskContext) -> String {
    let mut s = format!("{}\n", ctx.prompt.trim());
    if !ctx.workspace.scope().is_empty() {
        s.push_str(&format!(
            "\nOnly change files in: {}.\n",
            ctx.workspace.scope().join(", ")
        ));
    }
    if let Some(t) = &ctx.tests {
        s.push_str(&format!(
            "\nForge runs the tests ({}) after your turn and reports failures back to you.\n",
            t.command.display()
        ));
    }
    s
}

/// The run-test-fix loop around an agent CLI working in the sandbox: after each turn the
/// tests run; failures go back to the CLI until they pass or the budget is used up.
pub async fn run_cli(
    turns: &dyn CliTurns,
    ctx: &TaskContext,
    host: &dyn Host,
    cancel: &CancellationToken,
) -> Result<Outcome, String> {
    let mut message = cli_prompt(ctx);
    let mut last = None;
    let mut test_runs = 0;
    for _ in 0..ctx.budget.max_steps.max(1) {
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        let summary = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("cancelled".into()),
            r = turns.turn(&message) => r?,
        };
        let Some(spec) = &ctx.tests else {
            return Ok(Outcome {
                kind: OutcomeKind::Completed,
                summary,
                tests: None,
            });
        };
        let result = run_tests(spec, &ctx.budget, host).await;
        test_runs += 1;
        if result.green {
            return Ok(Outcome {
                kind: OutcomeKind::Completed,
                summary,
                tests: Some(result),
            });
        }
        if test_runs >= ctx.budget.max_test_runs {
            return Ok(Outcome {
                kind: OutcomeKind::TestsFailing,
                summary,
                tests: Some(result),
            });
        }
        message = format!(
            "The tests fail after your changes.\n{}\nFix the failures.",
            failure_report(&result)
        );
        last = Some(result);
    }
    Ok(Outcome {
        kind: OutcomeKind::BudgetExhausted,
        summary: None,
        tests: last,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use forge_ai::{AiResult, Completion, DeltaStream, ModelInfo};
    use forge_ai_editor::AiIgnore;
    use forge_output::run::TestReport;

    use super::*;

    /// Answers with scripted replies and records the requests.
    struct FakeProvider {
        replies: Mutex<VecDeque<String>>,
        seen: Mutex<Vec<CompletionRequest>>,
    }

    impl FakeProvider {
        fn new(replies: &[&str]) -> Self {
            Self {
                replies: Mutex::new(replies.iter().map(|s| s.to_string()).collect()),
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl Provider for FakeProvider {
        fn name(&self) -> &str {
            "fake"
        }
        fn default_model(&self) -> Option<&str> {
            Some("fake-1")
        }
        async fn list_models(&self) -> AiResult<Vec<ModelInfo>> {
            Ok(Vec::new())
        }
        async fn complete(&self, request: CompletionRequest) -> AiResult<Completion> {
            self.seen.lock().unwrap().push(request);
            let text = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| "<tool>{\"tool\":\"list_files\",\"args\":{}}</tool>".into());
            Ok(Completion {
                text,
                model: "fake-1".into(),
                stop_reason: None,
                usage: None,
            })
        }
        async fn stream(&self, _request: CompletionRequest) -> AiResult<DeltaStream> {
            unimplemented!()
        }
    }

    /// Runs the "tests" by checking a file, records commands, answers approvals.
    struct FakeHost {
        root: std::path::PathBuf,
        approve: bool,
        steps: Mutex<Vec<StepKind>>,
        ran: Mutex<Vec<String>>,
    }

    impl FakeHost {
        fn new(root: &std::path::Path, approve: bool) -> Self {
            Self {
                root: root.into(),
                approve,
                steps: Mutex::new(Vec::new()),
                ran: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl Host for FakeHost {
        async fn run(&self, cmd: &ShellCommand, _timeout: Duration) -> RunOutput {
            self.ran.lock().unwrap().push(cmd.display());
            assert_eq!(cmd.cwd, self.root);
            if cmd.program == "test-runner" {
                let code = std::fs::read_to_string(self.root.join("src/sum.txt")).unwrap();
                let ok = code.trim() == "a + b";
                let out = if ok {
                    "##teamcity[testStarted name='testSum']\n##teamcity[testFinished name='testSum']\n"
                } else {
                    "##teamcity[testStarted name='testSum']\n##teamcity[testFailed name='testSum' message='expected 5, got -1']\n##teamcity[testFinished name='testSum']\n"
                };
                return RunOutput {
                    exit_code: Some(if ok { 0 } else { 1 }),
                    output: out.into(),
                    timed_out: false,
                };
            }
            RunOutput {
                exit_code: Some(0),
                output: "ran".into(),
                timed_out: false,
            }
        }
        async fn approve(&self, _step: usize, _command: &str) -> bool {
            self.approve
        }
        fn log(&self, kind: StepKind) -> usize {
            let mut s = self.steps.lock().unwrap();
            s.push(kind);
            s.len() - 1
        }
        fn update(&self, index: usize, kind: StepKind) {
            self.steps.lock().unwrap()[index] = kind;
        }
    }

    fn context(root: &std::path::Path, tests: bool, budget: Budget) -> TaskContext {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/sum.txt"), "a - b\n").unwrap();
        TaskContext {
            prompt: "Fix the sum".into(),
            workspace: Workspace::new(root, AiIgnore::default(), vec!["src".into()]),
            allowlist: vec!["php -l".into()],
            tests: tests.then(|| TestSpec {
                label: "Unit tests".into(),
                command: ShellCommand {
                    program: "test-runner".into(),
                    cwd: root.into(),
                    ..Default::default()
                },
                report: Some(TestReport::TeamCity),
            }),
            budget,
        }
    }

    fn call(tool: &str, args: serde_json::Value) -> String {
        format!(
            "Next step.\n<tool>{}</tool>",
            serde_json::json!({ "tool": tool, "args": args })
        )
    }

    #[tokio::test]
    async fn fixes_until_the_tests_pass() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path(), true, Budget::default());
        let host = FakeHost::new(dir.path(), true);
        let provider = FakeProvider::new(&[
            &call("read_file", serde_json::json!({ "path": "src/sum.txt" })),
            &call("run_tests", serde_json::json!({})),
            // A wrong fix first: finish runs the tests and sends the agent back.
            &call(
                "edit_file",
                serde_json::json!({ "path": "src/sum.txt", "old": "a - b", "new": "a * b" }),
            ),
            &call("finish", serde_json::json!({ "summary": "Fixed" })),
            &call(
                "edit_file",
                serde_json::json!({ "path": "src/sum.txt", "old": "a * b", "new": "a + b" }),
            ),
            &call("finish", serde_json::json!({ "summary": "Fixed the sum" })),
        ]);
        let out = run_builtin(&provider, None, &ctx, &host, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out.kind, OutcomeKind::Completed);
        assert_eq!(out.summary.as_deref(), Some("Fixed the sum"));
        assert!(out.tests.unwrap().green);
        assert_eq!(host.ran.lock().unwrap().len(), 3);
        let seen = provider.seen.lock().unwrap();
        assert!(seen[0].system.as_deref().unwrap().contains("run_tests"));
        assert!(seen[1]
            .messages
            .last()
            .unwrap()
            .text_content()
            .contains("a - b"));
        assert!(seen[4]
            .messages
            .last()
            .unwrap()
            .text_content()
            .contains("expected 5, got -1"));
        let steps = host.steps.lock().unwrap();
        assert!(steps
            .iter()
            .any(|s| matches!(s, StepKind::Tests { result } if result.green)));
    }

    #[tokio::test]
    async fn stops_at_the_step_budget() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget {
            max_steps: 3,
            ..Budget::default()
        };
        let ctx = context(dir.path(), false, budget);
        let host = FakeHost::new(dir.path(), true);
        let provider = FakeProvider::new(&["I am thinking without tools."]);
        let out = run_builtin(&provider, None, &ctx, &host, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out.kind, OutcomeKind::BudgetExhausted);
        assert_eq!(provider.seen.lock().unwrap().len(), 3);
        // The model was told how to call tools.
        let seen = provider.seen.lock().unwrap();
        assert!(seen[1]
            .messages
            .last()
            .unwrap()
            .text_content()
            .contains("No tool call"));
    }

    #[tokio::test]
    async fn commands_need_approval_unless_allowlisted() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path(), false, Budget::default());
        let host = FakeHost::new(dir.path(), false);
        let provider = FakeProvider::new(&[
            &call(
                "run_command",
                serde_json::json!({ "command": "php -l src/sum.txt" }),
            ),
            &call("run_command", serde_json::json!({ "command": "rm -r src" })),
            &call(
                "run_command",
                serde_json::json!({ "command": "curl http://x" }),
            ),
            &call(
                "write_file",
                serde_json::json!({ "path": "README.md", "content": "x" }),
            ),
            &call("read_file", serde_json::json!({ "path": "../outside" })),
            &call("finish", serde_json::json!({ "summary": "" })),
        ]);
        let out = run_builtin(&provider, None, &ctx, &host, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out.kind, OutcomeKind::Completed);
        assert_eq!(out.summary, None);
        assert_eq!(*host.ran.lock().unwrap(), ["php -l src/sum.txt"]);
        let steps = host.steps.lock().unwrap();
        assert!(steps.iter().any(|s| matches!(
            s,
            StepKind::Approval { state: ApprovalState::Denied, command, .. } if command == "rm -r src"
        )));
        let errors: Vec<&String> = steps
            .iter()
            .filter_map(|s| match s {
                StepKind::Tool {
                    output,
                    is_error: true,
                    ..
                } => Some(output),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 4);
        assert!(errors[0].contains("denied"));
        assert!(errors[1].contains("no network tools"));
        assert!(errors[2].contains("scope"));
        assert!(errors[3].contains("outside the sandbox"));
        assert!(!dir.path().join("README.md").exists());
    }

    #[tokio::test]
    async fn cancels() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path(), false, Budget::default());
        let host = FakeHost::new(dir.path(), true);
        let provider = FakeProvider::new(&[]);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = run_builtin(&provider, None, &ctx, &host, &cancel)
            .await
            .unwrap_err();
        assert_eq!(err, "cancelled");
    }

    /// A CLI that fixes the file on its second turn.
    struct FakeCli {
        root: std::path::PathBuf,
        messages: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl CliTurns for FakeCli {
        async fn turn(&self, message: &str) -> Result<Option<String>, String> {
            let mut m = self.messages.lock().unwrap();
            m.push(message.into());
            let text = if m.len() == 1 { "a * b" } else { "a + b" };
            std::fs::write(self.root.join("src/sum.txt"), text).unwrap();
            Ok(Some(format!("turn {}", m.len())))
        }
    }

    #[tokio::test]
    async fn cli_runs_tests_between_turns() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path(), true, Budget::default());
        let host = FakeHost::new(dir.path(), true);
        let cli = FakeCli {
            root: dir.path().into(),
            messages: Mutex::new(Vec::new()),
        };
        let out = run_cli(&cli, &ctx, &host, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out.kind, OutcomeKind::Completed);
        assert_eq!(out.summary.as_deref(), Some("turn 2"));
        {
            let m = cli.messages.lock().unwrap();
            assert!(m[0].contains("Only change files in: src"));
            assert!(m[0].contains("Forge runs the tests (test-runner)"));
            assert!(m[1].contains("expected 5, got -1"));
        }

        // Without passing tests the test budget ends the loop.
        let budget = Budget {
            max_test_runs: 1,
            ..Budget::default()
        };
        let ctx = context(dir.path(), true, budget);
        let cli = FakeCli {
            root: dir.path().into(),
            messages: Mutex::new(Vec::new()),
        };
        let out = run_cli(&cli, &ctx, &host, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out.kind, OutcomeKind::TestsFailing);
        assert_eq!(out.tests.unwrap().failed, 1);
    }
}
