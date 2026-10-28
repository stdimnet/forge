//! The task model shown in the Agent Tasks tool window: what was asked, which agent works on
//! it, its status and the log of steps. Tasks of a project are stored as JSON next to their
//! sandboxes in Forge's cache folder ([`TaskStore`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where a task is in its life cycle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    /// Waiting for an earlier task of the project to finish.
    #[default]
    Queued,
    Running,
    /// The agent is done; its changes wait in the sandbox to be applied or discarded.
    NeedsReview,
    /// The changes were applied to the working tree (or committed / put on a branch).
    Applied,
    /// The sandbox was removed without applying anything.
    Discarded,
    /// The agent stopped with an error; what it changed so far can still be reviewed.
    Failed,
    /// Stopped by the user; what it changed so far can still be reviewed.
    Cancelled,
}

impl TaskStatus {
    /// Whether the task still has a sandbox with changes to review.
    pub fn reviewable(self) -> bool {
        matches!(self, Self::NeedsReview | Self::Failed | Self::Cancelled)
    }

    /// Whether the task is over (nothing runs and nothing waits).
    pub fn is_final(self) -> bool {
        matches!(self, Self::Applied | Self::Discarded)
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

/// Which agent works on a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AgentBackend {
    /// Forge's own tool-calling loop over a model of Settings | Tools | AI Providers.
    Builtin {
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },
    /// An agent CLI (`claude-code`, `gemini-cli`) started headless inside the sandbox.
    Cli {
        agent_id: String,
        #[serde(default)]
        model: Option<String>,
    },
}

impl Default for AgentBackend {
    fn default() -> Self {
        Self::Builtin {
            provider: None,
            model: None,
        }
    }
}

impl AgentBackend {
    pub fn label(&self) -> String {
        match self {
            Self::Builtin { model, .. } => match model {
                Some(m) if !m.is_empty() => format!("Forge agent ({m})"),
                _ => "Forge agent".into(),
            },
            Self::Cli { agent_id, .. } => match agent_id.as_str() {
                "claude-code" => "Claude Code".into(),
                "gemini-cli" => "Gemini CLI".into(),
                other => other.into(),
            },
        }
    }
}

/// How the agent checks its work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TestChoice {
    /// No tests: the task ends when the agent says it is done.
    #[default]
    None,
    /// A run configuration of the project (its test runner and result format).
    Configuration { name: String },
    /// A command line, run in the sandbox root.
    Command { command: String },
}

/// What the user asked for (the "New Agent Task" form).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskRequest {
    pub prompt: String,
    /// Project-relative files and folders the agent may change; empty: the whole project.
    pub scope: Vec<String>,
    pub backend: AgentBackend,
    pub tests: TestChoice,
    /// Overrides `ai.agentTasks.maxSteps`.
    pub max_steps: Option<u32>,
}

/// How a finished run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeKind {
    /// The agent finished (and the tests pass, when there are tests).
    Completed,
    /// The step budget ran out first.
    BudgetExhausted,
    /// The test runs were used up and the tests still fail.
    TestsFailing,
}

/// The result of an agent run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub kind: OutcomeKind,
    /// The agent's own summary of what it did.
    pub summary: Option<String>,
    /// The last test run.
    pub tests: Option<TestResult>,
}

/// One test run, evaluated.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub green: bool,
    pub exit_code: Option<i32>,
    pub passed: u32,
    pub failed: u32,
    /// `name: message` of failed tests (at most a few).
    pub failures: Vec<String>,
    /// The end of the output (what the agent sees when no test results were parsed).
    pub output_tail: String,
}

/// State of a command waiting for the user's approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalState {
    Pending,
    Allowed,
    Denied,
}

/// One entry of a task's log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StepKind {
    /// Progress written by Forge (sandbox created, tests started, ...).
    Info {
        text: String,
    },
    /// What the agent said or planned.
    Thought {
        text: String,
    },
    /// A tool call and its result.
    Tool {
        tool: String,
        /// One line for the log, e.g. `src/Order.php` or `composer test`.
        summary: String,
        input: Value,
        output: String,
        is_error: bool,
    },
    /// A command that needs the user's approval (answered by the step's index).
    Approval {
        command: String,
        state: ApprovalState,
    },
    /// A test run.
    Tests {
        result: TestResult,
    },
    Error {
        message: String,
    },
    /// The run ended.
    Finished {
        outcome: Outcome,
    },
}

/// A numbered log entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub index: usize,
    /// Unix milliseconds.
    pub at: u64,
    #[serde(flatten)]
    pub kind: StepKind,
}

/// A task as stored and shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTask {
    pub id: String,
    /// First line of the prompt, shortened.
    pub title: String,
    pub request: TaskRequest,
    pub status: TaskStatus,
    /// Unix milliseconds.
    pub created: u64,
    pub updated: u64,
    /// The sandbox (a git worktree in Forge's cache folder), once created.
    #[serde(default)]
    pub sandbox: Option<PathBuf>,
    /// The commit the sandbox started from (the diff to review is against it).
    #[serde(default)]
    pub base_commit: Option<String>,
    #[serde(default)]
    pub outcome: Option<Outcome>,
    /// Why the task failed.
    #[serde(default)]
    pub error: Option<String>,
    /// What applying did: the branch or commit it created.
    #[serde(default)]
    pub applied: Option<String>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

impl AgentTask {
    pub fn new(id: impl Into<String>, request: TaskRequest, now: u64) -> Self {
        Self {
            id: id.into(),
            title: title_of(&request.prompt),
            request,
            status: TaskStatus::Queued,
            created: now,
            updated: now,
            sandbox: None,
            base_commit: None,
            outcome: None,
            error: None,
            applied: None,
            steps: Vec::new(),
        }
    }

    /// Append a step; returns its index.
    pub fn push(&mut self, kind: StepKind, now: u64) -> usize {
        let index = self.steps.len();
        self.steps.push(Step {
            index,
            at: now,
            kind,
        });
        self.updated = now;
        index
    }

    /// Replace a step (an approval that was answered).
    pub fn replace(&mut self, index: usize, kind: StepKind, now: u64) {
        if let Some(s) = self.steps.get_mut(index) {
            s.kind = kind;
            self.updated = now;
        }
    }
}

/// The task title: the prompt's first non-empty line, at most 80 characters.
pub fn title_of(prompt: &str) -> String {
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("Untitled task");
    if line.chars().count() > 80 {
        let cut: String = line.chars().take(79).collect();
        format!("{}…", cut.trim_end())
    } else {
        line.to_string()
    }
}

/// The tasks of one project, saved as `tasks.json` in its folder of Forge's cache.
pub struct TaskStore {
    dir: PathBuf,
}

impl TaskStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The folder holding `tasks.json` and the sandboxes.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the sandbox of task `id` goes.
    pub fn sandbox_dir(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    fn file(&self) -> PathBuf {
        self.dir.join("tasks.json")
    }

    /// Stored tasks; tasks that were running when Forge quit are marked cancelled.
    pub fn load(&self) -> Vec<AgentTask> {
        let Ok(text) = std::fs::read_to_string(self.file()) else {
            return Vec::new();
        };
        let mut tasks: Vec<AgentTask> = serde_json::from_str(&text).unwrap_or_default();
        for t in &mut tasks {
            if t.status.is_active() {
                t.status = if t.sandbox.is_some() {
                    TaskStatus::Cancelled
                } else {
                    TaskStatus::Discarded
                };
            }
        }
        tasks
    }

    pub fn save(&self, tasks: &[AgentTask]) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let text = serde_json::to_string_pretty(tasks).map_err(std::io::Error::other)?;
        let tmp = self.dir.join("tasks.json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, self.file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_steps() {
        assert_eq!(
            title_of("\n  Fix the failing test\nmore"),
            "Fix the failing test"
        );
        assert_eq!(title_of(""), "Untitled task");
        assert!(title_of(&"x".repeat(200)).ends_with('…'));
        let mut t = AgentTask::new("t1", TaskRequest::default(), 1);
        let i = t.push(
            StepKind::Approval {
                command: "make".into(),
                state: ApprovalState::Pending,
            },
            2,
        );
        t.replace(
            i,
            StepKind::Approval {
                command: "make".into(),
                state: ApprovalState::Allowed,
            },
            3,
        );
        assert_eq!(t.updated, 3);
        let v = serde_json::to_value(&t.steps[0]).unwrap();
        assert_eq!(v["kind"], "approval");
        assert_eq!(v["state"], "allowed");
        assert!(TaskStatus::Failed.reviewable());
        assert!(!TaskStatus::Applied.reviewable());
    }

    #[test]
    fn request_json_and_labels() {
        let r: TaskRequest = serde_json::from_str(
            r#"{"prompt":"p","backend":{"kind":"cli","agentId":"claude-code"},"tests":{"kind":"command","command":"composer test"}}"#,
        )
        .unwrap();
        assert_eq!(r.backend.label(), "Claude Code");
        assert_eq!(
            r.tests,
            TestChoice::Command {
                command: "composer test".into()
            }
        );
        assert_eq!(AgentBackend::default().label(), "Forge agent");
    }

    #[test]
    fn store_round_trip_cancels_running_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let store = TaskStore::new(dir.path().join("p"));
        let mut a = AgentTask::new("a", TaskRequest::default(), 1);
        a.status = TaskStatus::Running;
        a.sandbox = Some(store.sandbox_dir("a"));
        let mut b = AgentTask::new("b", TaskRequest::default(), 1);
        b.status = TaskStatus::Queued;
        let mut c = AgentTask::new("c", TaskRequest::default(), 1);
        c.status = TaskStatus::NeedsReview;
        store.save(&[a, b, c]).unwrap();
        let tasks = store.load();
        let status: Vec<TaskStatus> = tasks.iter().map(|t| t.status).collect();
        assert_eq!(
            status,
            [
                TaskStatus::Cancelled,
                TaskStatus::Discarded,
                TaskStatus::NeedsReview
            ]
        );
        assert!(TaskStore::new(dir.path().join("none")).load().is_empty());
    }
}
