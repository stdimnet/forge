/**
 * Agent Tasks tool window: the project's tasks with their status, a form for new tasks, and
 * the selected task's log of steps, command approvals and the changes to review (diff per
 * file, Apply, Commit, Branch, Discard).
 */
import { createResource, createSignal, For, Match, onMount, Show, Switch } from "solid-js";
import {
  Ban,
  Check,
  CircleAlert,
  CircleCheck,
  CircleDashed,
  CirclePause,
  CircleX,
  FlaskConical,
  GitBranch,
  GitCommitHorizontal,
  Info,
  ListChecks,
  LoaderCircle,
  MessageSquareText,
  Plus,
  ShieldQuestion,
  Square,
  Trash2,
  Undo2,
  Wrench,
} from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { aiApi } from "@/ipc/ai";
import { runApi } from "@/ipc/run";
import { isActive, isReviewable, type AgentTask, type Step, type TaskRequest, type TaskStatus, type TestChoice } from "@/ipc/agentTasks";
import { setInputDialog } from "@/features/git/overlays";
import {
  applyAnswer,
  applyTask,
  approve,
  busy,
  cancelTask,
  changes,
  createTask,
  discardTask,
  error,
  formOpen,
  loadTasks,
  removeTask,
  select,
  selectedId,
  selectedTask,
  setError,
  setFormOpen,
  showChange,
  tasks,
} from "./store";
import "./agentTasks.css";

const STATUS_LABEL: Record<TaskStatus, string> = {
  queued: "Queued",
  running: "Running",
  needsReview: "Needs review",
  applied: "Applied",
  discarded: "Discarded",
  failed: "Failed",
  cancelled: "Cancelled",
};

function StatusIcon(props: { status: TaskStatus; size?: number }) {
  const size = () => props.size ?? 14;
  return (
    <Switch>
      <Match when={props.status === "queued"}>
        <CircleDashed size={size()} class="at-status-queued" />
      </Match>
      <Match when={props.status === "running"}>
        <LoaderCircle size={size()} class="at-status-running at-spin" />
      </Match>
      <Match when={props.status === "needsReview"}>
        <ShieldQuestion size={size()} class="at-status-review" />
      </Match>
      <Match when={props.status === "applied"}>
        <CircleCheck size={size()} class="at-status-applied" />
      </Match>
      <Match when={props.status === "discarded"}>
        <Ban size={size()} class="at-status-discarded" />
      </Match>
      <Match when={props.status === "failed"}>
        <CircleX size={size()} class="at-status-failed" />
      </Match>
      <Match when={props.status === "cancelled"}>
        <CirclePause size={size()} class="at-status-discarded" />
      </Match>
    </Switch>
  );
}

function ago(ms: number): string {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return new Date(ms).toLocaleDateString();
}

function backendLabel(t: AgentTask): string {
  const b = t.request.backend;
  if (b.kind === "cli") return b.agentId === "gemini-cli" ? "Gemini CLI" : "Claude Code";
  return b.model ? `Forge agent (${b.model})` : "Forge agent";
}

function testsLabel(c: TestChoice): string {
  if (c.kind === "command") return c.command;
  if (c.kind === "configuration") return c.name;
  return "No tests";
}

/** Header buttons of the tool window. */
export function AgentTasksActions() {
  return (
    <button class="icon-btn" title="New Agent Task" onClick={() => setFormOpen(true)}>
      <Plus size={15} />
    </button>
  );
}

function NewTaskForm() {
  const [prompt, setPrompt] = createSignal("");
  const [scope, setScope] = createSignal("");
  const [agent, setAgent] = createSignal("builtin:");
  const [tests, setTests] = createSignal("none");
  const [command, setCommand] = createSignal("");
  const [providers] = createResource(() => aiApi.providers().catch(() => null));
  const [configs] = createResource(() => runApi.loadConfigurations().catch(() => null));
  let area!: HTMLTextAreaElement;
  onMount(() => area.focus());

  const submit = () => {
    const [kind, id] = agent().split(":");
    const t = tests();
    const choice: TestChoice =
      t === "none" ? { kind: "none" } : t === "command" ? { kind: "command", command: command().trim() } : { kind: "configuration", name: t.slice("cfg:".length) };
    const request: TaskRequest = {
      prompt: prompt(),
      scope: scope()
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
      backend: kind === "cli" ? { kind: "cli", agentId: id } : { kind: "builtin", provider: id || null },
      tests: choice,
    };
    void createTask(request);
  };

  return (
    <div class="at-form">
      <div class="at-form-title">New Agent Task</div>
      <label class="at-label" for="at-prompt">
        Task
      </label>
      <textarea
        id="at-prompt"
        ref={area}
        class="at-input at-prompt"
        placeholder="Describe what the agent should do, e.g. “Fix the failing CheckoutTest”"
        value={prompt()}
        onInput={(e) => setPrompt(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            submit();
          }
        }}
      />
      <label class="at-label" for="at-scope">
        Files it may change
      </label>
      <input
        id="at-scope"
        class="at-input"
        placeholder="Whole project (or e.g. app/Services, tests/Unit)"
        value={scope()}
        onInput={(e) => setScope(e.currentTarget.value)}
      />
      <label class="at-label" for="at-agent">
        Agent
      </label>
      <select id="at-agent" class="at-input" value={agent()} onChange={(e) => setAgent(e.currentTarget.value)}>
        <option value="builtin:">Forge agent (default AI provider)</option>
        <For each={(providers()?.providers ?? []).filter((p) => p.enabled)}>{(p) => <option value={`builtin:${p.id}`}>Forge agent: {p.name}</option>}</For>
        <option value="cli:claude-code">Claude Code</option>
        <option value="cli:gemini-cli">Gemini CLI</option>
      </select>
      <label class="at-label" for="at-tests">
        Verify with
      </label>
      <select id="at-tests" class="at-input" value={tests()} onChange={(e) => setTests(e.currentTarget.value)}>
        <option value="none">No tests</option>
        <For each={configs()?.configurations ?? []}>{(c) => <option value={`cfg:${c.name}`}>{c.name}</option>}</For>
        <option value="command">Command…</option>
      </select>
      <Show when={tests() === "command"}>
        <input class="at-input" placeholder="vendor/bin/phpunit" value={command()} onInput={(e) => setCommand(e.currentTarget.value)} />
      </Show>
      <p class="at-hint muted">
        The agent works in a copy of the project (a git worktree in Forge's cache folder). Your files change only when you apply its diff.
      </p>
      <div class="at-form-buttons">
        <button class="btn-secondary" onClick={() => setFormOpen(false)}>
          Cancel
        </button>
        <button class="btn-primary" disabled={!prompt().trim() || busy() || (tests() === "command" && !command().trim())} onClick={submit} title="Start (Ctrl+Enter)">
          Start
        </button>
      </div>
    </div>
  );
}

function StepRow(props: { task: AgentTask; step: Step }) {
  const [open, setOpen] = createSignal(false);
  const s = props.step;
  return (
    <div class={`at-step at-step-${s.kind}`}>
      <Switch>
        <Match when={s.kind === "info" && s}>
          {(st) => (
            <div class="at-step-line muted">
              <Info size={13} />
              <span>{st().text}</span>
            </div>
          )}
        </Match>
        <Match when={s.kind === "thought" && s}>
          {(st) => (
            <div class="at-step-line">
              <MessageSquareText size={13} class="at-accent" />
              <span class="at-thought">{st().text}</span>
            </div>
          )}
        </Match>
        <Match when={s.kind === "tool" && s}>
          {(st) => (
            <>
              <button class="at-step-line at-tool" onClick={() => setOpen(!open())} title={open() ? "Hide output" : "Show output"}>
                <Wrench size={13} class={st().isError ? "at-status-failed" : "muted"} />
                <span class="at-tool-name">{st().tool}</span>
                <span class="at-tool-summary">{st().summary}</span>
                <Show when={st().isError}>
                  <CircleAlert size={12} class="at-status-failed" />
                </Show>
              </button>
              <Show when={open() && st().output}>
                <pre class="at-output">{st().output}</pre>
              </Show>
            </>
          )}
        </Match>
        <Match when={s.kind === "approval" && s}>
          {(st) => (
            <div class="at-approval">
              <div class="at-step-line">
                <ShieldQuestion size={13} class="at-status-review" />
                <span>{st().state === "pending" ? "Run this command?" : st().state === "allowed" ? "Allowed" : "Denied"}</span>
              </div>
              <code class="at-command">{st().command}</code>
              <Show when={st().state === "pending"}>
                <div class="at-approval-buttons">
                  <button class="btn-primary" onClick={() => void approve(props.task.id, s.index, true)}>
                    Allow
                  </button>
                  <button class="btn-secondary" onClick={() => void approve(props.task.id, s.index, false)}>
                    Deny
                  </button>
                </div>
              </Show>
            </div>
          )}
        </Match>
        <Match when={s.kind === "tests" && s}>
          {(st) => (
            <>
              <button class="at-step-line at-tool" onClick={() => setOpen(!open())}>
                <FlaskConical size={13} class={st().result.green ? "at-status-applied" : "at-status-failed"} />
                <span class={st().result.green ? "at-status-applied" : "at-status-failed"}>
                  {st().result.green ? "Tests passed" : "Tests failed"}
                </span>
                <span class="muted">
                  {st().result.passed} passed{st().result.failed ? `, ${st().result.failed} failed` : ""}
                </span>
              </button>
              <For each={st().result.failures}>{(f) => <div class="at-failure">{f}</div>}</For>
              <Show when={open()}>
                <pre class="at-output">{st().result.outputTail}</pre>
              </Show>
            </>
          )}
        </Match>
        <Match when={s.kind === "error" && s}>
          {(st) => (
            <div class="at-step-line at-status-failed">
              <CircleX size={13} />
              <span>{st().message}</span>
            </div>
          )}
        </Match>
        <Match when={s.kind === "finished" && s}>
          {(st) => (
            <div class="at-finished">
              <div class="at-step-line">
                <Check size={13} class={st().outcome.kind === "completed" ? "at-status-applied" : "at-status-review"} />
                <strong>
                  {st().outcome.kind === "completed" ? "Done" : st().outcome.kind === "budgetExhausted" ? "Stopped: step budget used up" : "Stopped: tests still fail"}
                </strong>
              </div>
              <Show when={st().outcome.summary}>
                <div class="at-summary">{st().outcome.summary}</div>
              </Show>
            </div>
          )}
        </Match>
      </Switch>
    </div>
  );
}

function Review(props: { task: AgentTask }) {
  const id = () => props.task.id;
  const commit = () =>
    setInputDialog({
      title: "Apply as Commit",
      label: "Commit message:",
      value: props.task.title,
      okLabel: "Commit",
      onOk: (message) => applyTask(id(), { mode: "commit", message }),
    });
  const branch = () =>
    setInputDialog({
      title: "Apply as Branch",
      label: "New branch name:",
      value: `agent/${props.task.title
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-|-$/g, "")
        .slice(0, 40)}`,
      okLabel: "Create Branch",
      onOk: (name) => applyTask(id(), { mode: "branch", branch: name, message: props.task.title }),
    });
  return (
    <div class="at-review">
      <div class="at-section-title">
        Changes
        <span class="muted">{changes() ? ` ${changes()!.length} file${changes()!.length === 1 ? "" : "s"}` : ""}</span>
      </div>
      <Show when={applyAnswer()}>
        {(a) => (
          <div class={`at-banner ${a().applied ? "at-banner-ok" : "at-banner-warn"}`}>
            {a().applied ? `Applied ${a().files.length} file${a().files.length === 1 ? "" : "s"}${a().created ? `: ${a().created}` : ""}` : a().message}
            <Show when={a().conflicts.length}>
              <div>Conflicts: {a().conflicts.join(", ")}</div>
            </Show>
          </div>
        )}
      </Show>
      <Show when={changes()?.length} fallback={<div class="muted at-empty">{changes() ? "The agent changed no files." : "Loading changes…"}</div>}>
        <div class="at-files">
          <For each={changes()}>
            {(c) => (
              <div class="tree-row at-file" onDblClick={() => void showChange(id(), c.path)} title="Show Diff (double-click)">
                <FileIcon name={c.path.split("/").pop() ?? c.path} size={14} />
                <span class={`at-file-name at-kind-${c.kind}`}>{c.path.split("/").pop()}</span>
                <span class="muted at-file-dir">{c.path.includes("/") ? c.path.slice(0, c.path.lastIndexOf("/")) : ""}</span>
                <span class="at-flex" />
                <Show when={!c.binary} fallback={<span class="muted">binary</span>}>
                  <span class="at-add">+{c.additions}</span>
                  <span class="at-del">−{c.deletions}</span>
                </Show>
                <button class="icon-btn" title="Apply this file only" onClick={() => void applyTask(id(), { mode: "apply" }, [c.path])}>
                  <Check size={13} />
                </button>
                <button class="icon-btn" title="Show Diff" onClick={() => void showChange(id(), c.path)}>
                  <ListChecks size={13} />
                </button>
              </div>
            )}
          </For>
        </div>
        <div class="at-review-buttons">
          <button class="btn-primary" disabled={busy()} onClick={() => void applyTask(id(), { mode: "apply" })} title="Patch the working tree (all or nothing)">
            Apply
          </button>
          <button class="btn-secondary" disabled={busy()} onClick={commit} title="Apply and commit the changed files">
            <GitCommitHorizontal size={13} /> Commit…
          </button>
          <button class="btn-secondary" disabled={busy()} onClick={branch} title="Commit on a new branch; the working tree stays as it is">
            <GitBranch size={13} /> Branch…
          </button>
        </div>
      </Show>
    </div>
  );
}

function Details(props: { task: AgentTask }) {
  const t = () => props.task;
  return (
    <div class="at-details">
      <div class="at-details-head">
        <StatusIcon status={t().status} size={16} />
        <span class="at-details-title">{t().title}</span>
      </div>
      <div class="at-meta muted">
        <span>{STATUS_LABEL[t().status]}</span>
        <span>{backendLabel(t())}</span>
        <span>
          <FlaskConical size={11} /> {testsLabel(t().request.tests)}
        </span>
        <Show when={t().request.scope.length}>
          <span>Scope: {t().request.scope.join(", ")}</span>
        </Show>
        <Show when={t().applied}>
          <span>{t().applied}</span>
        </Show>
      </div>
      <div class="at-details-buttons">
        <Show when={isActive(t().status)}>
          <button class="btn-secondary" onClick={() => void cancelTask(t().id)}>
            <Square size={12} /> Stop
          </button>
        </Show>
        <Show when={isReviewable(t().status)}>
          <button class="btn-secondary" disabled={busy()} onClick={() => void discardTask(t().id)} title="Remove the sandbox without applying anything">
            <Undo2 size={12} /> Discard
          </button>
        </Show>
        <Show when={!isActive(t().status)}>
          <button class="btn-secondary" disabled={busy()} onClick={() => void removeTask(t().id)} title="Remove the task from the list">
            <Trash2 size={12} /> Remove
          </button>
        </Show>
      </div>
      <Show when={isReviewable(t().status) && t().sandbox}>
        <Review task={t()} />
      </Show>
      <div class="at-section-title">Steps</div>
      <div class="at-steps">
        <For each={t().steps}>{(s) => <StepRow task={t()} step={s} />}</For>
        <Show when={t().status === "running"}>
          <div class="at-step-line muted">
            <LoaderCircle size={13} class="at-spin" /> Working…
          </div>
        </Show>
      </div>
    </div>
  );
}

export function AgentTasksPanel() {
  onMount(() => void loadTasks());
  return (
    <div class="at-panel">
      <Show when={error()}>
        <div class="at-banner at-banner-warn" onClick={() => setError(null)}>
          {error()}
        </div>
      </Show>
      <Show when={formOpen()}>
        <NewTaskForm />
      </Show>
      <Show
        when={tasks().length}
        fallback={
          <Show when={!formOpen()}>
            <div class="at-empty-state">
              <ListChecks size={28} class="muted" />
              <div>No agent tasks yet</div>
              <div class="muted">An agent works on a copy of the project, runs the tests and leaves a diff for you to review.</div>
              <button class="btn-primary" onClick={() => setFormOpen(true)}>
                New Agent Task
              </button>
            </div>
          </Show>
        }
      >
        <div class="at-list" role="listbox" aria-label="Agent tasks">
          <For each={tasks()}>
            {(t) => (
              <div
                class="tree-row at-row"
                classList={{ selected: t.id === selectedId() }}
                role="option"
                aria-selected={t.id === selectedId()}
                onClick={() => select(t.id)}
              >
                <StatusIcon status={t.status} />
                <span class="at-row-title">{t.title}</span>
                <span class="at-flex" />
                <span class="muted at-row-meta">{STATUS_LABEL[t.status]}</span>
                <span class="muted at-row-meta">{ago(t.created)}</span>
              </div>
            )}
          </For>
        </div>
        <Show when={selectedTask()}>{(t) => <Details task={t()} />}</Show>
      </Show>
    </div>
  );
}
