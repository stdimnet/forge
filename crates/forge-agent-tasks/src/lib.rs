//! Agent tasks: an AI agent works on a task in a sandbox (a git worktree in Forge's cache
//! folder), runs the tests and fixes failures, and leaves a diff for the user to review and
//! apply. The user's working tree is not touched until then.
//!
//! - [`task`]: the task model of the Agent Tasks tool window (status, steps) and its store.
//! - [`sandbox`]: creating the worktree from a snapshot of the working tree, the changed
//!   files, and Apply (all or nothing), Apply as Commit, Apply as Branch and Discard.
//! - [`workspace`]: path confinement, `.aiignore` and scope checks plus the file tools.
//! - [`tools`]: the built-in agent's tools and the text protocol to call them.
//! - [`commands`]: command lines without a shell, the allowlist, refused network programs,
//!   running processes and evaluating test runs with Forge's test output parsers.
//! - [`agent`]: the loops: the built-in tool-calling agent over a `forge-ai` provider and
//!   the run-test-fix loop around an agent CLI.
//! - [`cli`]: permission requests of agent CLIs mapped to the sandbox.
//! - [`container`]: the container sandbox: `docker run` / `exec` / `rm` command lines with
//!   only the sandbox mounted, no network by default and resource limits.
//! - [`remote`]: sandbox operations as backend protocol calls, answered where the project's
//!   files are (the desktop app, or the Forge agent on a remote project's host).
//!
//! Without the default `agent` feature (the Forge agent on a remote host) the run loops and
//! local processes are left out.

#[cfg(feature = "agent")]
pub mod agent;
pub mod cli;
pub mod commands;
pub mod container;
pub mod remote;
pub mod sandbox;
pub mod task;
pub mod tools;
pub mod workspace;

#[cfg(feature = "agent")]
pub use agent::{run_builtin, run_cli, Budget, CliTurns, Host, TaskContext};
pub use cli::{permission, CliVerdict};
pub use commands::{RunOutput, ShellCommand, TestSpec};
pub use container::{ContainerOptions, ContainerSpec};
pub use remote::{ApplyAnswer, ApplyMode, FileVersions, SandboxCall, SandboxInfo, SandboxRef};
pub use sandbox::{ApplyResult, ChangeKind, FileChange, Sandbox};
pub use task::{
    AgentBackend, AgentTask, ApprovalState, Outcome, OutcomeKind, Step, StepKind, TaskRequest,
    TaskStatus, TaskStore, TestChoice, TestResult,
};
pub use workspace::Workspace;
