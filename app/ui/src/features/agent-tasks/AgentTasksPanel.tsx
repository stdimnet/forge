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
  Container,
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
import { t as tr } from "@/i18n";
import { getSetting } from "@/state/settings";
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

const statusLabel = (s: TaskStatus): string => tr(`tools.agentTasks.status.${s}`);

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
  if (s < 60) return tr("tools.agentTasks.ago.now");
  if (s < 3600) return tr("tools.agentTasks.ago.minutes", { count: Math.round(s / 60) });
  if (s < 86400) return tr("tools.agentTasks.ago.hours", { count: Math.round(s / 3600) });
  return new Date(ms).toLocaleDateString();
}

function backendLabel(t: AgentTask): string {
  const b = t.request.backend;
  if (b.kind === "cli") return b.agentId === "gemini-cli" ? "Gemini CLI" : "Claude Code";
  return b.model ? tr("tools.agentTasks.agent.forgeModel", { model: b.model }) : tr("tools.agentTasks.agent.forge");
}

function testsLabel(c: TestChoice): string {
  if (c.kind === "command") return c.command;
  if (c.kind === "configuration") return c.name;
  return tr("tools.agentTasks.tests.none");
}

/** Header buttons of the tool window. */
export function AgentTasksActions() {
  return (
    <button class="icon-btn" data-testid="at-new" title={tr("tools.agentTasks.new")} onClick={() => setFormOpen(true)}>
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
  const [inContainer, setInContainer] = createSignal(getSetting<boolean>("ai.agentTasks.container.enabled", false));
  const [network, setNetwork] = createSignal(getSetting<boolean>("ai.agentTasks.container.network", false));
  const [image, setImage] = createSignal("");
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
      container: inContainer() ? { network: network(), image: image().trim() || null } : null,
    };
    void createTask(request);
  };

  return (
    <div class="at-form">
      <div class="at-form-title">{tr("tools.agentTasks.new")}</div>
      <label class="at-label" for="at-prompt">
        {tr("tools.agentTasks.form.task")}
      </label>
      <textarea
        id="at-prompt"
        ref={area}
        class="at-input at-prompt"
        placeholder={tr("tools.agentTasks.form.taskPlaceholder")}
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
        {tr("tools.agentTasks.form.scope")}
      </label>
      <input
        id="at-scope"
        class="at-input"
        placeholder={tr("tools.agentTasks.form.scopePlaceholder")}
        value={scope()}
        onInput={(e) => setScope(e.currentTarget.value)}
      />
      <label class="at-label" for="at-agent">
        {tr("tools.agentTasks.form.agent")}
      </label>
      <select id="at-agent" class="at-input" value={agent()} onChange={(e) => setAgent(e.currentTarget.value)}>
        <option value="builtin:">{tr("tools.agentTasks.form.agentDefault")}</option>
        <For each={(providers()?.providers ?? []).filter((p) => p.enabled)}>{(p) => <option value={`builtin:${p.id}`}>{tr("tools.agentTasks.form.agentProvider", { name: p.name })}</option>}</For>
        <option value="cli:claude-code">Claude Code</option>
        <option value="cli:gemini-cli">Gemini CLI</option>
      </select>
      <label class="at-label" for="at-tests">
        {tr("tools.agentTasks.form.verify")}
      </label>
      <select id="at-tests" class="at-input" value={tests()} onChange={(e) => setTests(e.currentTarget.value)}>
        <option value="none">{tr("tools.agentTasks.tests.none")}</option>
        <For each={configs()?.configurations ?? []}>{(c) => <option value={`cfg:${c.name}`}>{c.name}</option>}</For>
        <option value="command">{tr("tools.agentTasks.form.command")}</option>
      </select>
      <Show when={tests() === "command"}>
        <input class="at-input" placeholder="vendor/bin/phpunit" value={command()} onInput={(e) => setCommand(e.currentTarget.value)} />
      </Show>
      <label class="at-check" title={tr("tools.agentTasks.form.containerHint")}>
        <input type="checkbox" data-testid="at-container" checked={inContainer()} onChange={(e) => setInContainer(e.currentTarget.checked)} />
        <Container size={13} />
        {tr("tools.agentTasks.form.container")}
      </label>
      <Show when={inContainer()}>
        <div class="at-container-options">
          <label class="at-check">
            <input type="checkbox" data-testid="at-network" checked={network()} onChange={(e) => setNetwork(e.currentTarget.checked)} />
            {tr("tools.agentTasks.form.network")}
          </label>
          <label class="at-label" for="at-image">
            {tr("tools.agentTasks.form.image")}
          </label>
          <input
            id="at-image"
            class="at-input"
            placeholder={tr("tools.agentTasks.form.imagePlaceholder", { image: getSetting<string>("ai.agentTasks.container.image", "php:8.3-cli") })}
            value={image()}
            onInput={(e) => setImage(e.currentTarget.value)}
          />
          <p class="at-hint muted">{tr("tools.agentTasks.form.containerHint")}</p>
        </div>
      </Show>
      <p class="at-hint muted">
        {tr("tools.agentTasks.form.hint")}
      </p>
      <div class="at-form-buttons">
        <button class="btn-secondary" onClick={() => setFormOpen(false)}>
          {tr("common.cancel")}
        </button>
        <button class="btn-primary" disabled={!prompt().trim() || busy() || (tests() === "command" && !command().trim())} onClick={submit} title={tr("tools.agentTasks.form.startTitle", { shortcut: "Ctrl+Enter" })}>
          {tr("tools.agentTasks.form.start")}
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
              <button class="at-step-line at-tool" onClick={() => setOpen(!open())} title={open() ? tr("tools.agentTasks.step.hideOutput") : tr("tools.agentTasks.step.showOutput")}>
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
            <div class={`at-approval at-approval-${st().state}`}>
              <div class="at-step-line">
                <ShieldQuestion size={13} class="at-status-review" />
                <span>{st().state === "pending" ? tr("tools.agentTasks.approval.ask") : st().state === "allowed" ? tr("tools.agentTasks.approval.allowed") : tr("tools.agentTasks.approval.denied")}</span>
              </div>
              <code class="at-command">{st().command}</code>
              <Show when={st().state === "pending"}>
                <div class="at-approval-buttons">
                  <button class="btn-primary" onClick={() => void approve(props.task.id, s.index, true)}>
                    {tr("tools.agentTasks.approval.allow")}
                  </button>
                  <button class="btn-secondary" onClick={() => void approve(props.task.id, s.index, false)}>
                    {tr("tools.agentTasks.approval.deny")}
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
                  {st().result.green ? tr("tools.agentTasks.step.testsPassed") : tr("tools.agentTasks.step.testsFailed")}
                </span>
                <span class="muted">
                  {st().result.failed
                    ? tr("tools.agentTasks.step.passedFailed", { passed: st().result.passed, failed: st().result.failed })
                    : tr("tools.agentTasks.step.passed", { passed: st().result.passed })}
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
                  {tr(`tools.agentTasks.outcome.${st().outcome.kind}`)}
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
      title: tr("tools.agentTasks.review.commitTitle"),
      label: tr("tools.agentTasks.review.commitLabel"),
      value: props.task.title,
      okLabel: tr("tools.agentTasks.review.commitOk"),
      onOk: (message) => applyTask(id(), { mode: "commit", message }),
    });
  const branch = () =>
    setInputDialog({
      title: tr("tools.agentTasks.review.branchTitle"),
      label: tr("tools.agentTasks.review.branchLabel"),
      value: `agent/${props.task.title
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-|-$/g, "")
        .slice(0, 40)}`,
      okLabel: tr("tools.agentTasks.review.branchOk"),
      onOk: (name) => applyTask(id(), { mode: "branch", branch: name, message: props.task.title }),
    });
  return (
    <div class="at-review">
      <div class="at-section-title">
        {tr("tools.agentTasks.review.changes")}
        <span class="muted">{changes() ? ` ${tr("tools.agentTasks.review.files", { count: changes()!.length })}` : ""}</span>
      </div>
      <Show when={applyAnswer()}>
        {(a) => (
          <div class={`at-banner ${a().applied ? "at-banner-ok" : "at-banner-warn"}`}>
            {a().applied
              ? a().created
                ? tr("tools.agentTasks.review.appliedCreated", { count: a().files.length, created: a().created })
                : tr("tools.agentTasks.review.applied", { count: a().files.length })
              : a().message}
            <Show when={a().conflicts.length}>
              <div>{tr("tools.agentTasks.review.conflicts", { files: a().conflicts.join(", ") })}</div>
            </Show>
          </div>
        )}
      </Show>
      <Show when={changes()?.length} fallback={<div class="muted at-empty">{changes() ? tr("tools.agentTasks.review.noChanges") : tr("tools.agentTasks.review.loading")}</div>}>
        <div class="at-files">
          <For each={changes()}>
            {(c) => (
              <div class="tree-row at-file" onDblClick={() => void showChange(id(), c.path)} title={tr("tools.agentTasks.review.showDiffDouble")}>
                <FileIcon name={c.path.split("/").pop() ?? c.path} size={14} />
                <span class={`at-file-name at-kind-${c.kind}`}>{c.path.split("/").pop()}</span>
                <span class="muted at-file-dir">{c.path.includes("/") ? c.path.slice(0, c.path.lastIndexOf("/")) : ""}</span>
                <span class="at-flex" />
                <Show when={!c.binary} fallback={<span class="muted">{tr("tools.agentTasks.review.binary")}</span>}>
                  <span class="at-add">+{c.additions}</span>
                  <span class="at-del">−{c.deletions}</span>
                </Show>
                <button class="icon-btn" title={tr("tools.agentTasks.review.applyFile")} onClick={() => void applyTask(id(), { mode: "apply" }, [c.path])}>
                  <Check size={13} />
                </button>
                <button class="icon-btn" title={tr("tools.agentTasks.review.showDiff")} onClick={() => void showChange(id(), c.path)}>
                  <ListChecks size={13} />
                </button>
              </div>
            )}
          </For>
        </div>
        <div class="at-review-buttons">
          <button class="btn-primary" disabled={busy()} onClick={() => void applyTask(id(), { mode: "apply" })} title={tr("tools.agentTasks.review.applyTitle")}>
            {tr("tools.agentTasks.review.apply")}
          </button>
          <button class="btn-secondary" disabled={busy()} onClick={commit} title={tr("tools.agentTasks.review.commitButtonTitle")}>
            <GitCommitHorizontal size={13} /> {tr("tools.agentTasks.review.commitButton")}
          </button>
          <button class="btn-secondary" disabled={busy()} onClick={branch} title={tr("tools.agentTasks.review.branchButtonTitle")}>
            <GitBranch size={13} /> {tr("tools.agentTasks.review.branchButton")}
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
        <span>{statusLabel(t().status)}</span>
        <span>{backendLabel(t())}</span>
        <span>
          <FlaskConical size={11} /> {testsLabel(t().request.tests)}
        </span>
        <Show when={t().request.container}>
          {(c) => (
            <span title={tr("tools.agentTasks.form.containerHint")}>
              <Container size={11} /> {c().network ? tr("tools.agentTasks.meta.containerNetwork") : tr("tools.agentTasks.meta.container")}
            </span>
          )}
        </Show>
        <Show when={t().request.scope.length}>
          <span>{tr("tools.agentTasks.meta.scope", { scope: t().request.scope.join(", ") })}</span>
        </Show>
        <Show when={t().applied}>
          <span>{t().applied}</span>
        </Show>
      </div>
      <div class="at-details-buttons">
        <Show when={isActive(t().status)}>
          <button class="btn-secondary" onClick={() => void cancelTask(t().id)}>
            <Square size={12} /> {tr("tools.agentTasks.stop")}
          </button>
        </Show>
        <Show when={isReviewable(t().status)}>
          <button class="btn-secondary" disabled={busy()} onClick={() => void discardTask(t().id)} title={tr("tools.agentTasks.discardTitle")}>
            <Undo2 size={12} /> {tr("tools.agentTasks.discard")}
          </button>
        </Show>
        <Show when={!isActive(t().status)}>
          <button class="btn-secondary" disabled={busy()} onClick={() => void removeTask(t().id)} title={tr("tools.agentTasks.removeTitle")}>
            <Trash2 size={12} /> {tr("tools.agentTasks.remove")}
          </button>
        </Show>
      </div>
      <Show when={isReviewable(t().status) && t().sandbox}>
        <Review task={t()} />
      </Show>
      <div class="at-section-title">{tr("tools.agentTasks.steps")}</div>
      <div class="at-steps">
        <For each={t().steps}>{(s) => <StepRow task={t()} step={s} />}</For>
        <Show when={t().status === "running"}>
          <div class="at-step-line muted">
            <LoaderCircle size={13} class="at-spin" /> {tr("tools.agentTasks.working")}
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
              <div>{tr("tools.agentTasks.empty")}</div>
              <div class="muted">{tr("tools.agentTasks.emptyHint")}</div>
              <button class="btn-primary" onClick={() => setFormOpen(true)}>
                {tr("tools.agentTasks.new")}
              </button>
            </div>
          </Show>
        }
      >
        <div class="at-list" role="listbox" aria-label={tr("tools.agentTasks.listLabel")}>
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
                <span class="muted at-row-meta">{statusLabel(t.status)}</span>
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
