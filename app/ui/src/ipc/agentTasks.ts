/**
 * Typed client for Agent Tasks (crates/forge-agent-tasks via
 * app/src-tauri/src/commands/agent_tasks.rs): agents that work in a git worktree sandbox,
 * run the tests and fix failures, and leave a diff to review and apply.
 */
import { invoke, listen } from "./index";

export type TaskStatus = "queued" | "running" | "needsReview" | "applied" | "discarded" | "failed" | "cancelled";

export type AgentBackend =
  | { kind: "builtin"; provider?: string | null; model?: string | null }
  | { kind: "cli"; agentId: string; model?: string | null };

export type TestChoice = { kind: "none" } | { kind: "configuration"; name: string } | { kind: "command"; command: string };

/** "Run in container": commands and tests run in a Docker / Podman container. */
export interface ContainerOptions {
  /** Allow network access (every command then asks for approval). */
  network: boolean;
  /** Image for this task; empty: the project's devcontainer.json, else the settings' image. */
  image?: string | null;
}

export interface TaskRequest {
  prompt: string;
  /** Project-relative files and folders the agent may change; empty: the whole project. */
  scope: string[];
  backend: AgentBackend;
  tests: TestChoice;
  maxSteps?: number | null;
  /** Run in a container; absent: on the project's machine. */
  container?: ContainerOptions | null;
}

export interface TestResult {
  green: boolean;
  exitCode: number | null;
  passed: number;
  failed: number;
  failures: string[];
  outputTail: string;
}

export type OutcomeKind = "completed" | "budgetExhausted" | "testsFailing";

export interface Outcome {
  kind: OutcomeKind;
  summary: string | null;
  tests: TestResult | null;
}

export type ApprovalState = "pending" | "allowed" | "denied";

export type StepKind =
  | { kind: "info"; text: string }
  | { kind: "thought"; text: string }
  | { kind: "tool"; tool: string; summary: string; input: unknown; output: string; isError: boolean }
  | { kind: "approval"; command: string; state: ApprovalState }
  | { kind: "tests"; result: TestResult }
  | { kind: "error"; message: string }
  | { kind: "finished"; outcome: Outcome };

export type Step = StepKind & { index: number; at: number };

export interface AgentTask {
  id: string;
  title: string;
  request: TaskRequest;
  status: TaskStatus;
  created: number;
  updated: number;
  sandbox?: string | null;
  baseCommit?: string | null;
  outcome?: Outcome | null;
  error?: string | null;
  applied?: string | null;
  /** The task's container while it may exist. */
  container?: string | null;
  /** The sandbox lives on the remote project's host. */
  remote?: boolean;
  steps: Step[];
}

export type ChangeKind = "added" | "modified" | "deleted";

export interface FileChange {
  path: string;
  kind: ChangeKind;
  additions: number;
  deletions: number;
  binary: boolean;
}

export interface FileDiff {
  before: string | null;
  after: string | null;
}

export type ApplyMode = { mode: "apply" } | { mode: "commit"; message: string } | { mode: "branch"; branch: string; message: string };

export interface ApplyAnswer {
  applied: boolean;
  files: string[];
  conflicts: string[];
  message: string | null;
  created: string | null;
}

export const agentTasksApi = {
  list: () => invoke<AgentTask[]>("agent_tasks_list"),
  create: (request: TaskRequest) => invoke<AgentTask>("agent_task_create", { request }),
  cancel: (id: string) => invoke<void>("agent_task_cancel", { id }),
  approve: (id: string, step: number, allow: boolean) => invoke<void>("agent_task_approve", { id, step, allow }),
  changes: (id: string) => invoke<FileChange[]>("agent_task_changes", { id }),
  fileDiff: (id: string, path: string) => invoke<FileDiff>("agent_task_file_diff", { id, path }),
  apply: (id: string, mode: ApplyMode, paths?: string[]) => invoke<ApplyAnswer>("agent_task_apply", { id, mode, paths }),
  discard: (id: string) => invoke<void>("agent_task_discard", { id }),
  remove: (id: string) => invoke<void>("agent_task_remove", { id }),
  /** Every change of a task (status, a new step) with the whole task. */
  onChanged: (cb: (task: AgentTask) => void) => listen<AgentTask>("agent-tasks://changed", cb),
};

export const isReviewable = (s: TaskStatus) => s === "needsReview" || s === "failed" || s === "cancelled";
export const isActive = (s: TaskStatus) => s === "queued" || s === "running";
