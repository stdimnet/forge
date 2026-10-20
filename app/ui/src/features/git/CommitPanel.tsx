/** Non-modal Commit tool window (Alt+0 / Ctrl+K), laid out like PHPStorm's. */
import { createEffect, createMemo, createRoot, createSignal, For, on, Show } from "solid-js";
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Clock,
  FileDiff,
  Folder,
  LoaderCircle,
  RefreshCw,
  Settings,
  Sparkles,
  Undo2,
} from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { openFile } from "@/state/editor";
import { workspaceApi } from "@/ipc/workspace";
import { displayStatus, gitApi, type StatusEntry } from "./api";
import { notify, repositoryChanged, status } from "./store";
import { commitChanges, confirmRollback, showDiff } from "./actions";
import { generateCommitMessage } from "./aiMessage";
import { unifiedDiff } from "./textDiff";

// Panel state lives at module level so it survives hiding the tool window.
const [message, setMessage] = createSignal("");
const [amend, setAmend] = createSignal(false);
const [checked, setChecked] = createSignal<Set<string>>(new Set());
const [known, setKnown] = createSignal<Set<string>>(new Set());
const [selected, setSelected] = createSignal<string | null>(null);
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
const [generating, setGenerating] = createSignal(false);
const [busy, setBusy] = createSignal(false);

export { setMessage as setCommitMessage, message as commitMessage };

const changes = () => (status()?.entries ?? []).filter((e) => e.worktree !== "untracked" && e.worktree !== "ignored");
const unversioned = () => (status()?.entries ?? []).filter((e) => e.worktree === "untracked");

// New tracked changes start checked, unversioned files unchecked (PHPStorm default).
createRoot(() =>
  createEffect(
  on(status, (st) => {
    if (!st) return;
    const prev = known();
    const next = new Set(checked());
    const all = new Set(st.entries.map((e) => e.path));
    for (const e of st.entries) if (!prev.has(e.path) && e.worktree !== "untracked") next.add(e.path);
    for (const p of next) if (!all.has(p)) next.delete(p);
    setKnown(all);
    setChecked(next);
  }),
  ),
);

const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const plural = (n: number) => `${n} file${n === 1 ? "" : "s"}`;

function toggle(paths: string[], on: boolean) {
  setChecked((prev) => {
    const next = new Set(prev);
    for (const p of paths) on ? next.add(p) : next.delete(p);
    return next;
  });
}

function toggleCollapsed(id: string) {
  setCollapsed((prev) => {
    const next = new Set(prev);
    next.has(id) ? next.delete(id) : next.add(id);
    return next;
  });
}

function Checkbox(props: { paths: string[] }) {
  const count = () => props.paths.filter((p) => checked().has(p)).length;
  let ref!: HTMLInputElement;
  createEffect(() => (ref.indeterminate = count() > 0 && count() < props.paths.length));
  return (
    <input
      ref={ref}
      type="checkbox"
      class="commit-check"
      checked={props.paths.length > 0 && count() === props.paths.length}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => toggle(props.paths, e.currentTarget.checked)}
    />
  );
}

function FileRow(props: { entry: StatusEntry; depth: number; showDir: boolean }) {
  const st = () => displayStatus(props.entry);
  return (
    <div
      class="tree-row commit-row"
      classList={{ selected: selected() === props.entry.path }}
      style={{ "padding-left": `${8 + props.depth * 18}px` }}
      data-path={props.entry.path}
      onClick={() => setSelected(props.entry.path)}
      onDblClick={() => (st() === "untracked" ? void openFile(props.entry.path) : void showDiff(props.entry.path, allPaths()))}
      title={props.entry.path}
    >
      <span class="tree-chevron" />
      <Checkbox paths={[props.entry.path]} />
      <FileIcon name={nameOf(props.entry.path)} />
      <span class={`tree-label vcs-${st()}`}>{nameOf(props.entry.path)}</span>
      <Show when={props.showDir && dirOf(props.entry.path)}>
        <span class="tree-hint">{dirOf(props.entry.path)}</span>
      </Show>
    </div>
  );
}

/** One changelist node ("Changes", "Unversioned Files") with files grouped by directory. */
function Group(props: { id: string; title: string; entries: StatusEntry[]; strong?: boolean }) {
  const open = () => !collapsed().has(props.id);
  const dirs = createMemo(() => {
    const map = new Map<string, StatusEntry[]>();
    for (const e of [...props.entries].sort((a, b) => a.path.localeCompare(b.path))) {
      const d = dirOf(e.path);
      map.set(d, [...(map.get(d) ?? []), e]);
    }
    return [...map.entries()];
  });
  return (
    <Show when={props.entries.length > 0}>
      <div class="tree-row commit-group" onClick={() => toggleCollapsed(props.id)} style={{ "padding-left": "8px" }}>
        <span class="tree-chevron">{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>
        <Checkbox paths={props.entries.map((e) => e.path)} />
        <span class="tree-label" classList={{ strong: props.strong }}>
          {props.title}
        </span>
        <span class="tree-hint">{plural(props.entries.length)}</span>
      </div>
      <Show when={open()}>
        <For each={dirs()}>
          {([dir, files]) => (
            <Show
              when={dir && files.length > 1}
              fallback={<For each={files}>{(e) => <FileRow entry={e} depth={1} showDir />}</For>}
            >
              <div class="tree-row" style={{ "padding-left": "26px" }} onClick={() => toggleCollapsed(`${props.id}:${dir}`)}>
                <span class="tree-chevron">
                  {collapsed().has(`${props.id}:${dir}`) ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
                </span>
                <Checkbox paths={files.map((e) => e.path)} />
                <Folder size={15} stroke-width={1.75} class="muted" />
                <span class="tree-label">{dir}</span>
                <span class="tree-hint">{plural(files.length)}</span>
              </div>
              <Show when={!collapsed().has(`${props.id}:${dir}`)}>
                <For each={files}>{(e) => <FileRow entry={e} depth={2} showDir={false} />}</For>
              </Show>
            </Show>
          )}
        </For>
      </Show>
    </Show>
  );
}

const allPaths = () => [...changes(), ...unversioned()].map((e) => e.path);
const checkedPaths = () => allPaths().filter((p) => checked().has(p));

async function generate() {
  const paths = checkedPaths();
  if (!paths.length) {
    notify("info", "Select files to generate a commit message for");
    return;
  }
  setGenerating(true);
  try {
    const entries = (status()?.entries ?? []).filter((e) => paths.includes(e.path));
    const tracked = entries.filter((e) => e.worktree !== "untracked").map((e) => e.path);
    let diff = tracked.length ? await gitApi.diffHead(tracked) : "";
    for (const e of entries.filter((x) => x.worktree === "untracked"))
      diff += unifiedDiff(e.path, null, await workspaceApi.readFile(e.path).catch(() => ""));
    const files = entries.map((e) => ({ path: e.path, status: displayStatus(e) }));
    setMessage(await generateCommitMessage(diff, files, status()?.branch));
  } catch (err) {
    notify("error", "Could not generate commit message", String(err instanceof Error ? err.message : err), 10000);
  } finally {
    setGenerating(false);
  }
}

async function doCommit(pushAfter: boolean) {
  if (busy()) return;
  setBusy(true);
  try {
    const ok = await commitChanges({ message: message(), amend: amend(), paths: checkedPaths(), push: pushAfter });
    if (ok) {
      setMessage("");
      setAmend(false);
    }
  } finally {
    setBusy(false);
  }
}

async function toggleAmend(on: boolean) {
  setAmend(on);
  if (on && !message().trim()) {
    const [last] = await gitApi.log({ limit: 1 }).catch(() => []);
    if (last) {
      const details = await gitApi.commitDetails(last.sha).catch(() => null);
      setMessage(details?.message ?? last.subject);
    }
  }
}

export function CommitPanel() {
  const canCommit = () => !busy() && (checkedPaths().length > 0 || amend()) && message().trim().length > 0;
  const total = () => changes().length + unversioned().length;
  const onKey = (e: KeyboardEvent) => {
    if (e.key.toLowerCase() === "d" && (e.ctrlKey || e.metaKey) && selected()) {
      e.preventDefault();
      void showDiff(selected()!, allPaths());
    }
  };
  return (
    <div class="commit-panel" onKeyDown={onKey} tabIndex={-1}>
      <div class="panel-toolbar">
        <button class="icon-btn" title="Refresh (Ctrl+F5)" onClick={() => void repositoryChanged()}>
          <RefreshCw size={15} />
        </button>
        <button
          class="icon-btn"
          title="Rollback… (Ctrl+Alt+Z)"
          disabled={!checkedPaths().length}
          onClick={() => confirmRollback(checkedPaths().filter((p) => changes().some((c) => c.path === p)))}
        >
          <Undo2 size={15} />
        </button>
        <button
          class="icon-btn"
          title="Show Diff (Ctrl+D)"
          disabled={!selected() && !checkedPaths().length}
          onClick={() => void showDiff(selected() ?? checkedPaths()[0], allPaths())}
        >
          <FileDiff size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title="Expand All (Ctrl+NumPad +)" onClick={() => setCollapsed(new Set<string>())}>
          <ChevronsUpDown size={15} />
        </button>
        <button
          class="icon-btn"
          title="Collapse All (Ctrl+NumPad -)"
          onClick={() => setCollapsed(new Set(["changes", "unversioned"]))}
        >
          <ChevronsDownUp size={15} />
        </button>
      </div>
      <div class="commit-tree" role="tree">
        <Show
          when={total() > 0}
          fallback={<div class="commit-empty">{status() ? "No changes" : "No Git repository for this project"}</div>}
        >
          <Group id="changes" title="Changes" entries={changes()} strong />
          <Group id="unversioned" title="Unversioned Files" entries={unversioned()} />
        </Show>
      </div>
      <div class="commit-message">
        <div class="commit-message-toolbar">
          <Show when={status()?.branch}>
            <span class="commit-branch-hint" title="Current branch">
              {status()!.branch}
              <Show when={(status()?.ahead ?? 0) > 0}>
                <span class="muted"> ↑{status()!.ahead}</span>
              </Show>
            </span>
          </Show>
          <span class="spacer" />
          <button class="icon-btn" title="Commit Message History (Ctrl+M)">
            <Clock size={14} />
          </button>
          <button
            class="icon-btn ai-generate"
            classList={{ busy: generating() }}
            title="Generate Commit Message with AI"
            disabled={generating()}
            onClick={() => void generate()}
          >
            <Show when={generating()} fallback={<Sparkles size={15} />}>
              <LoaderCircle size={15} class="spin" />
            </Show>
          </button>
        </div>
        <textarea
          placeholder="Commit Message"
          value={message()}
          spellcheck={false}
          onInput={(e) => setMessage(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              if (canCommit()) void doCommit(false);
            }
          }}
        />
        <div class="commit-options">
          <label class="commit-amend">
            <input type="checkbox" checked={amend()} onChange={(e) => void toggleAmend(e.currentTarget.checked)} />
            Amend
          </label>
          <Show when={generating()}>
            <span class="muted ai-status">Generating commit message…</span>
          </Show>
        </div>
        <div class="commit-actions">
          <button class="btn-primary" disabled={!canCommit()} onClick={() => void doCommit(false)} title="Commit (Ctrl+Enter)">
            {amend() ? "Amend Commit" : "Commit"}
          </button>
          <button class="btn-secondary" disabled={!canCommit()} onClick={() => void doCommit(true)} title="Commit and Push… (Ctrl+Alt+K)">
            {amend() ? "Amend Commit and Push…" : "Commit and Push…"}
          </button>
          <span class="spacer" />
          <button class="icon-btn" title="Commit Options">
            <Settings size={15} />
          </button>
        </div>
      </div>
    </div>
  );
}

/** Select a file in the commit tree (used by screenshots/commands). */
export function selectCommitFile(path: string): void {
  setSelected(path);
}
