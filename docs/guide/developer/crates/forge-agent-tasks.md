---
title: forge-agent-tasks
description: "Agent tasks: AI agents that run, test and fix code in a git worktree sandbox, with path confinement, a command allowlist, step budgets and the diff to review and apply."
order: 22
tags: [crate, rust, ai, agents, sandbox]
---

# forge-agent-tasks

`crates/forge-agent-tasks` — agent tasks: an AI agent works on a task in a sandbox (a git
worktree in Forge's cache folder), runs the tests, fixes failures and leaves a diff for the
user to review. The user's working tree is not touched until they apply it.

## Purpose

- **Task model** (`task.rs`): `AgentTask` with its `TaskRequest` (prompt, scope, backend,
  tests, step budget), `TaskStatus` (queued, running, needs review, applied, discarded,
  failed, cancelled), the log of `Step`s (info, thought, tool call, command approval, test
  run, error, finished) and the `Outcome` (completed, step budget exhausted, tests still
  failing). `TaskStore` keeps a project's tasks as `tasks.json` next to the sandboxes; tasks
  that were running when Forge quit load as cancelled.
- **Sandbox** (`sandbox.rs`): `create` adds a detached worktree of the project's repository
  in the given folder (refused inside the project), starting from `git stash create` (the
  working tree and index without touching them) or HEAD, copies untracked non-ignored files
  and commits that snapshot as the base, so the review shows only the agent's changes.
  `changes` lists the changed files (`--name-status` + `--numstat` against the base),
  `file_versions` gives both sides for the diff viewer, `patch` the binary patch.
  `apply` is all or nothing (`git apply --check` first; conflicting files are parsed from its
  errors), `apply_as_commit` applies and commits only the agent's files (`git commit --only`),
  `apply_as_branch` commits in the sandbox and creates a branch (shared refs, so it shows up
  in the repository at once), `remove` removes the worktree and its folder.
- **Workspace** (`workspace.rs`): the agent's view of the sandbox. `normalize` refuses absolute
  paths, `..` escapes and `.git`; `resolve` also refuses symlinks that lead out of the sandbox
  and files matched by `.aiignore`; writes are limited to the task's scope. File tools:
  `list` (breadth first, dependency folders not descended into), `read` (line ranges),
  `search` (case-insensitive), `write`, `edit` (unique match), `delete`.
- **Tools** (`tools.rs`): the built-in agent's tools and the text protocol — one JSON call
  `{"tool": ..., "args": ...}` per turn inside a `<tool>` tag (fenced or bare JSON accepted),
  so it works with every chat model of [`forge-ai`](forge-ai.md); bad calls are explained to
  the model. `system_prompt` describes the tools, the scope and the test loop.
- **Commands** (`commands.rs`): `split_command` (quotes and escapes, shell operators refused —
  no shell runs), `allowlisted` (word prefixes from `ai.agentTasks.allowedCommands`),
  `refused` (network clients: curl, wget, ssh, scp, rsync, nc, ...), `run_process` (timeout,
  cancellation, `CI=1`) and `evaluate_tests`, which parses test output with the line parsers
  of [`forge-output`](forge-output.md) (TeamCity, TAP, libtest, go test2json, ...) or goes by
  the exit code.
- **Loops** (`agent.rs`): `run_builtin` asks a `forge_ai::Provider` for one tool call per turn,
  runs it in the sandbox and returns the result until the agent calls `finish`; with tests,
  `finish` is verified by a test run (when files changed since the last run) and failures are
  sent back until the tests pass or the step / test-run budget is used up. Commands that are
  not allowlisted wait for `Host::approve`. `run_cli` drives an agent CLI turn by turn
  (`CliTurns`) and runs the tests between turns, sending failures back.
- **CLI permissions** (`cli.rs`): `permission` maps a permission request of Claude Code or
  Gemini CLI to allow (reads inside the sandbox, edits inside the scope, bookkeeping tools),
  deny (outside the sandbox, `.aiignore`, network tools, unknown tools) or ask the user (shell
  commands that are not allowlisted).

The app (`app/src-tauri/src/commands/agent_tasks.rs`) owns the queue, the `Host` (processes,
approvals, the log as `agent-tasks://changed` events), maps run configurations into the
sandbox and starts agent CLIs through [`forge-agent-cli`](forge-agent-cli.md).

## Modules

| Module | Summary |
| --- | --- |
| `agent.rs` | The built-in tool-calling loop and the run-test-fix loop around agent CLIs. |
| `cli.rs` | Permission requests of agent CLIs mapped to the sandbox. |
| `commands.rs` | Command lines without a shell, allowlist, refused programs, processes, test results. |
| `sandbox.rs` | The git worktree sandbox: create, changes, diff sides, apply, commit, branch, remove. |
| `task.rs` | Task model, steps, outcomes and the task store. |
| `tools.rs` | Tool calls of the built-in agent, the text protocol and the system prompt. |
| `workspace.rs` | Path confinement, `.aiignore` and scope checks, file tools. |

## Main public items

```rust
pub use agent::{run_builtin, run_cli, Budget, CliTurns, Host, TaskContext};
pub use cli::{permission, CliVerdict};
pub use commands::{RunOutput, ShellCommand, TestSpec};
pub use sandbox::{ApplyResult, ChangeKind, FileChange, Sandbox};
pub use task::{AgentBackend, AgentTask, ApprovalState, Outcome, OutcomeKind, Step, StepKind, TaskRequest, TaskStatus, TaskStore, TestChoice, TestResult};
pub use workspace::Workspace;
```

## Used by

- IPC: `agent_tasks_list`, `agent_task_create`, `agent_task_cancel`, `agent_task_approve`,
  `agent_task_changes`, `agent_task_file_diff`, `agent_task_apply`, `agent_task_discard`,
  `agent_task_remove` (`app/src-tauri/src/commands/agent_tasks.rs`), event
  `agent-tasks://changed`
- UI: `app/ui/src/features/agent-tasks` (Agent Tasks tool window)
- Browser mode: `app/ui/src/mock/agentTasksMock.ts` replays a scripted task (nothing runs)

## Dependencies

- Workspace crates: [`forge-ai`](forge-ai.md) (the `Provider` trait), [`forge-ai-editor`](forge-ai-editor.md)
  (`.aiignore`), [`forge-output`](forge-output.md) (test output parsers), [`forge-vcs`](forge-vcs.md)
  (git binary, patch splitting)
- External: `serde`, `serde_json`, `tokio`, `async-trait`

## Tests

7 source files; 24 unit tests without network: a fake provider and a fake host drive the
built-in loop (fix until green, step budget, approvals and refusals, cancellation), a fake CLI
drives the run-test-fix loop, temporary git repositories cover the sandbox (snapshot of
uncommitted and untracked files, changes, partial apply, conflicts that change nothing, apply
as commit and as branch, removal), plus path confinement (symlinks, `.git`, `.aiignore`,
scope), command parsing, the allowlist, process timeouts, test evaluation and CLI permissions.

```sh
cargo test -p forge-agent-tasks
```
