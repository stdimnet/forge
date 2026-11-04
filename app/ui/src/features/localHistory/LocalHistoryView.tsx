/**
 * The Local History views, opened as editor tabs (PHPStorm opens a Local History window):
 * the revisions of a file or folder (Show History), of selected lines (Show History for
 * Selection) and the files of a Recent Changes entry. Revision list and details on the
 * left, the diff of the selected revision against the current version on the right, like
 * the Git history tabs.
 */
import { createEffect, createResource, createSignal, For, Match, on, onCleanup, Show, Switch, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  ArrowRightLeft,
  Columns2,
  Copy,
  FileDiff,
  FileMinus,
  FilePlus,
  GitBranch,
  History,
  Pencil,
  RefreshCw,
  Save,
  Search,
  Tag,
  TriangleAlert,
  Undo2,
  WandSparkles,
} from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { openCustomTab } from "@/state/editor";
import { item, separator, showContextMenu } from "@/features/contextMenu/state";
import { registerCompareHandler } from "@/features/compare/state";
import { setDiffRequest } from "@/features/git/overlays";
import { InlineDiff } from "@/features/git/InlineDiff";
import { localHistoryApi, type ChangeKind, type LhRevision, type StateKind } from "@/ipc/localHistory";
import "@/features/git/history.css";
import "./localHistory.css";
import {
  createPatch,
  currentText,
  historyDate,
  historyRevision,
  nameOf,
  putLabel,
  revertChange,
  revertTo,
  revisionText,
  revisionTitle,
  setHistoryTabOpener,
  showRevisionDiff,
  tabIdOf,
  tabTitleOf,
  type HistoryTarget,
} from "./actions";
import { t as tr } from "@/i18n";

/** The icon of a change kind. */
export const KIND_ICONS: Record<ChangeKind, (p: { size?: number; class?: string }) => JSX.Element> = {
  save: Save,
  edit: Pencil,
  refactoring: WandSparkles,
  external: TriangleAlert,
  vcs: GitBranch,
  create: FilePlus,
  delete: FileMinus,
  rename: ArrowRightLeft,
  move: ArrowRightLeft,
  copy: Copy,
  revert: Undo2,
  label: Tag,
  baseline: History,
};

/** Show Diff (Ctrl+D) of the Local History tab that last had the focus. */
let focusedShowDiff: (() => void) | null = null;
registerCompareHandler(".local-history", () => focusedShowDiff?.());

const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Up/Down in a list of ids. */
function listKeys(ids: () => (string | number)[], selected: () => string | number | null, select: (id: never) => void, root: () => HTMLElement) {
  return (e: KeyboardEvent) => {
    const list = ids();
    const i = list.indexOf(selected() ?? "");
    let next: string | number | undefined;
    if (e.key === "ArrowDown") next = list[Math.min(list.length - 1, i + 1)];
    else if (e.key === "ArrowUp") next = list[Math.max(0, i - 1)];
    else return;
    e.preventDefault();
    if (next !== undefined) {
      (select as (id: string | number) => void)(next);
      root().querySelector(`[data-rev="${next}"]`)?.scrollIntoView({ block: "nearest" });
    }
  };
}

function RevisionRow(props: {
  rev: { id: number; time: number; kind: ChangeKind; name: string };
  selected: boolean;
  /** Second column: the files of a folder revision, the path of a renamed file, the line. */
  hint?: string;
  onSelect: () => void;
  onMenu?: (e: MouseEvent) => void;
  onOpen: () => void;
}) {
  return (
    <div
      class="history-row lh-row"
      classList={{ selected: props.selected, "lh-label": props.rev.kind === "label" }}
      data-rev={props.rev.id}
      onClick={props.onSelect}
      onDblClick={props.onOpen}
      onContextMenu={(e) => {
        e.preventDefault();
        props.onSelect();
        props.onMenu?.(e);
      }}
    >
      <div class="history-subject">
        <Dynamic component={KIND_ICONS[props.rev.kind]} size={14} class={`lh-kind lh-kind-${props.rev.kind}`} />
        <span class="lh-name">{props.rev.name || tr("tools.localHistory.change")}</span>
        <Show when={props.hint}>
          <span class="history-renamed">{props.hint}</span>
        </Show>
      </div>
      <div class="log-date">{historyDate(props.rev.time)}</div>
    </div>
  );
}

function Filter(props: { value: string; onInput: (v: string) => void }) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  return (
    <label class="lh-filter" title={tr("tools.localHistory.showRevisionsWhoseNameOr")}>
      <Search size={13} class="muted" />
      <input
        type="text"
        placeholder={tr("tools.localHistory.filterByText")}
        value={props.value}
        spellcheck={false}
        onInput={(e) => {
          const v = e.currentTarget.value;
          clearTimeout(timer);
          timer = setTimeout(() => props.onInput(v), 250);
        }}
      />
    </label>
  );
}

const STATUS_CLASS: Record<string, string> = { added: "vcs-added", deleted: "vcs-deleted", modified: "vcs-modified" };

/** A file in a folder revision: how it differs from now. */
function statusOf(then: StateKind, now: StateKind): "added" | "deleted" | "modified" {
  if (then === "absent") return "added";
  if (now === "absent") return "deleted";
  return "modified";
}

function FileRow(props: { path: string; status: string; selected: boolean; title: string; onSelect: () => void; onOpen: () => void }) {
  return (
    <div
      class="tree-row"
      classList={{ selected: props.selected }}
      style={{ "padding-left": "10px" }}
      title={props.title}
      onClick={props.onSelect}
      onDblClick={props.onOpen}
    >
      <FileIcon name={nameOf(props.path)} />
      <span class={`tree-label ${STATUS_CLASS[props.status] ?? ""}`}>{nameOf(props.path)}</span>
      <span class="tree-hint">{props.path.split("/").slice(0, -1).join("/")}</span>
    </div>
  );
}

const STATUS_TITLE = {
  added: "Created after this revision (Revert deletes it)",
  deleted: "Deleted after this revision (Revert recreates it)",
  modified: "Changed after this revision",
};

// ---- file and folder history ------------------------------------------------------------------

function PathHistoryView(props: { target: Extract<HistoryTarget, { kind: "file" | "folder" }> }) {
  const folder = () => props.target.kind === "folder";
  const [filter, setFilter] = createSignal("");
  const [revs, { refetch }] = createResource(
    () => ({ t: props.target, f: filter(), n: historyRevision() }),
    ({ t, f }) => localHistoryApi.list(t.path, t.kind === "folder", f),
  );
  const [selected, setSelected] = createSignal<number | null>(null);
  const [selectedFile, setSelectedFile] = createSignal<string | null>(null);
  let list!: HTMLDivElement;
  createEffect(() => {
    const l = revs() ?? [];
    if (!l.some((r) => r.id === selected())) setSelected(l[0]?.id ?? null);
  });
  const current = (): LhRevision | undefined => revs()?.find((r) => r.id === selected());

  const [changes] = createResource(
    () => (folder() && current() ? { rev: current()!.id, n: historyRevision() } : null),
    ({ rev }) => localHistoryApi.folderChanges(props.target.path, rev),
  );
  /** The file shown in the diff: the tab's file, or the selected file of the folder revision. */
  const file = (): string | undefined => {
    if (!folder()) return props.target.path;
    const c = changes() ?? [];
    return (c.find((x) => x.path === selectedFile()) ?? c[0])?.path;
  };
  const revPath = () => (folder() ? file() : current()?.path);

  const [texts] = createResource(
    () => {
      const r = current();
      const f = file();
      const rp = revPath();
      return r && f && rp ? { r, f, rp, n: historyRevision() } : null;
    },
    async ({ r, f, rp }) => {
      const [then, now] = await Promise.all([revisionText(rp, r.id), currentText(f)]);
      return { r, then, now };
    },
  );

  const showDiff = () => {
    const r = current();
    const f = file();
    const rp = revPath();
    if (r && f && rp) void showRevisionDiff(f, rp, r);
  };
  const revert = () => {
    const r = current();
    if (r) void revertTo(props.target, r);
  };
  const patch = () => {
    const r = current();
    if (r) void createPatch(props.target, r);
  };
  const menu = (e: MouseEvent) =>
    showContextMenu(
      [
        item("Revert", revert, undefined, { disabled: !current() }),
        item("Show Diff", showDiff, "Compare.SameVersion", { shortcutAction: "CompareTwoFiles", disabled: !file() }),
        item("Create Patch…", patch, undefined, { disabled: !current() }),
        separator,
        item("Put Label…", () => void putLabel(), "LocalHistory.PutLabel"),
      ],
      e.clientX,
      e.clientY,
    );

  const emptyText = () => {
    if (revs.error) return errorText(revs.error);
    if (filter()) return "No revisions match the filter";
    return folder() ? "No local changes recorded in this folder yet" : "No local changes recorded for this file yet";
  };

  return (
    <div class="file-history local-history" onFocusIn={() => (focusedShowDiff = showDiff)} data-testid="local-history">
      <div class="file-history-main">
        <div class="panel-toolbar history-toolbar">
          <button class="icon-btn" title={tr("common.refresh")} onClick={() => void refetch()}>
            <RefreshCw size={15} />
          </button>
          <button class="icon-btn" title={tr("tools.localHistory.revert")} disabled={!current()} onClick={revert}>
            <Undo2 size={15} />
          </button>
          <button class="icon-btn" title={tr("tools.localHistory.showDiffCtrlD")} disabled={!file()} onClick={showDiff}>
            <Columns2 size={15} />
          </button>
          <button class="icon-btn" title={tr("tools.localHistory.createPatchEllipsis")} disabled={!current()} onClick={patch}>
            <FileDiff size={15} />
          </button>
          <button class="icon-btn" title={tr("tools.localHistory.putLabelEllipsis")} onClick={() => void putLabel()}>
            <Tag size={15} />
          </button>
          <span class="toolbar-sep" />
          <Filter value={filter()} onInput={setFilter} />
          <span class="spacer" />
          <span class="history-path" title={props.target.path}>
            {props.target.path || tr("common.project")}
          </span>
        </div>
        <div class="history-columns lh-columns">
          <div>{tr("tools.localHistory.change")}</div>
          <div>{tr("tools.localHistory.date")}</div>
        </div>
        <div
          class="log-list history-list"
          tabIndex={0}
          ref={list}
          onKeyDown={listKeys(
            () => (revs() ?? []).map((r) => r.id),
            selected,
            (id: number) => {
              setSelected(id);
              setSelectedFile(null);
            },
            () => list,
          )}
        >
          <For each={revs() ?? []}>
            {(r) => (
              <RevisionRow
                rev={r}
                selected={selected() === r.id}
                hint={
                  folder()
                    ? r.kind === "label"
                      ? undefined
                      : r.fileCount === 1
                        ? r.files[0]
                        : `${r.fileCount} files`
                    : r.path !== props.target.path
                      ? r.path
                      : undefined
                }
                onSelect={() => {
                  setSelected(r.id);
                  setSelectedFile(null);
                }}
                onMenu={menu}
                onOpen={showDiff}
              />
            )}
          </For>
          <Show when={revs() && !revs()!.length}>
            <div class="commit-empty">{emptyText()}</div>
          </Show>
        </div>
        <Show when={folder()}>
          <div class="history-details">
            <div class="lh-details-title">
              {current() ? tr("tools.localHistory.filesThatDifferFromThe", { value: changes()?.length ?? 0 }) : tr("tools.localHistory.selectARevision")}
            </div>
            <div class="history-files">
              <For each={changes() ?? []}>
                {(c) => {
                  const status = statusOf(c.then, c.now);
                  return (
                    <FileRow
                      path={c.path}
                      status={status}
                      title={STATUS_TITLE[status]}
                      selected={file() === c.path}
                      onSelect={() => setSelectedFile(c.path)}
                      onOpen={() => {
                        setSelectedFile(c.path);
                        showDiff();
                      }}
                    />
                  );
                }}
              </For>
              <Show when={current() && changes() && !changes()!.length}>
                <div class="commit-empty">{tr("tools.localHistory.noDifferencesTheFilesAre")}</div>
              </Show>
            </div>
          </div>
        </Show>
      </div>
      <Show
        when={texts()}
        keyed
        fallback={
          <div class="inline-diff">
            <div class="commit-empty">{current() ? (folder() && !file() ? tr("tools.localHistory.noDifferences") : tr("common.loadingEllipsis")) : tr("tools.localHistory.selectARevisionToView")}</div>
          </div>
        }
      >
        {(t) => (
          <InlineDiff
            path={file() ?? props.target.path}
            left={{ title: revisionTitle(t.r, t.then.state), content: t.then.text }}
            right={{ title: t.now.exists ? tr("tools.localHistory.currentVersion") : tr("tools.localHistory.currentVersionDeleted"), content: t.now.text }}
            empty={t.then.state === "unknown" ? "The content of this revision was not recorded (file too large, binary or never opened)" : undefined}
          />
        )}
      </Show>
    </div>
  );
}

// ---- selection history ------------------------------------------------------------------------

function SelectionHistoryView(props: { target: Extract<HistoryTarget, { kind: "selection" }> }) {
  const [history, { refetch }] = createResource(
    () => ({ t: props.target, n: historyRevision() }),
    async ({ t }) => {
      const now = await currentText(t.path);
      return localHistoryApi.selection(t.path, now.text, t.start, t.end);
    },
  );
  const [selected, setSelected] = createSignal<number | null>(null);
  let list!: HTMLDivElement;
  createEffect(() => {
    const l = history()?.revisions ?? [];
    if (!l.some((r) => r.id === selected())) setSelected(l[0]?.id ?? null);
  });
  const current = () => history()?.revisions.find((r) => r.id === selected());
  const lines = () => (props.target.start === props.target.end ? `line ${props.target.start}` : `lines ${props.target.start}–${props.target.end}`);
  const showDiff = () => {
    const r = current();
    const h = history();
    if (!r || !h) return;
    setDiffRequest({
      path: props.target.path,
      left: { title: tr("tools.localHistory.fromLine", { value: revisionTitle(r, "file"), startLine: r.startLine }), content: r.text },
      right: { title: tr("tools.localHistory.currentVersion2", { lines: lines() }), content: h.current },
      editable: false,
    });
  };

  return (
    <div class="file-history local-history" onFocusIn={() => (focusedShowDiff = showDiff)}>
      <div class="file-history-main">
        <div class="panel-toolbar history-toolbar">
          <button class="icon-btn" title={tr("common.refresh")} onClick={() => void refetch()}>
            <RefreshCw size={15} />
          </button>
          <button class="icon-btn" title={tr("tools.localHistory.showDiffCtrlD")} disabled={!current()} onClick={showDiff}>
            <Columns2 size={15} />
          </button>
          <span class="spacer" />
          <span class="history-path" title={props.target.path}>
            {props.target.path}, {lines()}
          </span>
        </div>
        <div class="history-columns lh-columns">
          <div>{tr("tools.localHistory.change")}</div>
          <div>{tr("tools.localHistory.date")}</div>
        </div>
        <div class="log-list history-list" tabIndex={0} ref={list} onKeyDown={listKeys(() => (history()?.revisions ?? []).map((r) => r.id), selected, setSelected as (id: never) => void, () => list)}>
          <For each={history()?.revisions ?? []}>
            {(r) => (
              <RevisionRow rev={r} selected={selected() === r.id} hint={r.text ? `line ${r.startLine}` : tr("tools.localHistory.linesDidNotExist")} onSelect={() => setSelected(r.id)} onOpen={showDiff} />
            )}
          </For>
          <Show when={history.error}>
            <div class="commit-empty">{errorText(history.error)}</div>
          </Show>
          <Show when={history() && !history()!.revisions.length}>
            <div class="commit-empty">{tr("tools.localHistory.noLocalChangesRecordedFor")}</div>
          </Show>
        </div>
      </div>
      <Show
        when={current() && history()}
        fallback={
          <div class="inline-diff">
            <div class="commit-empty">{tr("tools.localHistory.selectARevisionToView2")}</div>
          </div>
        }
      >
        <InlineDiff
          path={props.target.path}
          left={{ title: tr("tools.localHistory.fromLine", { value: revisionTitle(current()!, "file"), startLine: current()!.startLine }), content: current()!.text }}
          right={{ title: tr("tools.localHistory.currentVersion2", { lines: lines() }), content: history()!.current }}
        />
      </Show>
    </div>
  );
}

// ---- a Recent Changes entry -------------------------------------------------------------------

function ChangeView(props: { target: Extract<HistoryTarget, { kind: "change" }> }) {
  const [files, { refetch }] = createResource(
    () => ({ t: props.target, n: historyRevision() }),
    ({ t }) => localHistoryApi.changeFiles(t.from, t.to),
  );
  const [selected, setSelected] = createSignal<string | null>(null);
  createEffect(
    on(files, (l) => {
      if (l && !l.some((f) => f.path === selected())) setSelected(l[0]?.path ?? null);
    }),
  );
  const current = () => files()?.find((f) => f.path === selected());
  const [texts] = createResource(
    () => (current() ? { f: current()!, n: historyRevision() } : null),
    async ({ f }) => {
      const [before, after] = await Promise.all([revisionText(f.from ?? f.path, props.target.from), revisionText(f.path, props.target.to + 1)]);
      return { f, before, after };
    },
  );
  const sideTitle = (state: StateKind, when: string) =>
    state === "absent" ? `${when} (file did not exist)` : state === "unknown" ? `${when} (content not recorded)` : when;
  const showDiff = () => {
    const t = texts();
    if (!t) return;
    setDiffRequest({
      path: t.f.path,
      left: { title: sideTitle(t.before.state, `Before ${props.target.name}`), content: t.before.text },
      right: { title: sideTitle(t.after.state, `After ${props.target.name}`), content: t.after.text },
      editable: false,
    });
  };
  return (
    <div class="file-history local-history" onFocusIn={() => (focusedShowDiff = showDiff)}>
      <div class="file-history-main">
        <div class="panel-toolbar history-toolbar">
          <button class="icon-btn" title={tr("common.refresh")} onClick={() => void refetch()}>
            <RefreshCw size={15} />
          </button>
          <button class="icon-btn" title={tr("tools.localHistory.revert")} disabled={!files()?.length} onClick={() => void revertChange(props.target)}>
            <Undo2 size={15} />
          </button>
          <button class="icon-btn" title={tr("tools.localHistory.showDiffCtrlD")} disabled={!current()} onClick={showDiff}>
            <Columns2 size={15} />
          </button>
          <span class="spacer" />
          <span class="history-path">{historyDate(props.target.time)}</span>
        </div>
        <div class="lh-change-header">
          <span class="lh-name">{props.target.name}</span>
          <span class="muted">
            {tr("tools.localHistory.files", { count: files()?.length, value: files()?.length ?? 0 })}
          </span>
        </div>
        <div class="history-files lh-change-files">
          <For each={files() ?? []}>
            {(f) => {
              const status = f.before === "absent" ? "added" : f.after === "absent" ? "deleted" : "modified";
              return (
                <FileRow
                  path={f.path}
                  status={status}
                  title={f.from ? tr("tools.localHistory.movedFrom", { from: f.from }) : status === "added" ? tr("tools.localHistory.createdByThisChange") : status === "deleted" ? tr("tools.localHistory.deletedByThisChange") : tr("tools.localHistory.changed")}
                  selected={selected() === f.path}
                  onSelect={() => setSelected(f.path)}
                  onOpen={() => {
                    setSelected(f.path);
                    showDiff();
                  }}
                />
              );
            }}
          </For>
          <Show when={files.error}>
            <div class="commit-empty">{errorText(files.error)}</div>
          </Show>
        </div>
      </div>
      <Show
        when={texts()}
        keyed
        fallback={
          <div class="inline-diff">
            <div class="commit-empty">{files() && !files()!.length ? tr("tools.localHistory.thisChangeHasNoFiles") : tr("tools.localHistory.selectAFile")}</div>
          </div>
        }
      >
        {(t) => (
          <InlineDiff
            path={t.f.path}
            left={{ title: sideTitle(t.before.state, `Before: ${props.target.name}`), content: t.before.text }}
            right={{ title: sideTitle(t.after.state, t.after.state === "file" && files() ? `After: ${props.target.name}` : "After"), content: t.after.text }}
          />
        )}
      </Show>
    </div>
  );
}

export function LocalHistoryView(props: { target: HistoryTarget }) {
  return (
    <Switch>
      <Match when={props.target.kind === "selection" && props.target} keyed>
        {(t) => <SelectionHistoryView target={t as Extract<HistoryTarget, { kind: "selection" }>} />}
      </Match>
      <Match when={props.target.kind === "change" && props.target} keyed>
        {(t) => <ChangeView target={t as Extract<HistoryTarget, { kind: "change" }>} />}
      </Match>
      <Match when={(props.target.kind === "file" || props.target.kind === "folder") && props.target} keyed>
        {(t) => <PathHistoryView target={t as Extract<HistoryTarget, { kind: "file" | "folder" }>} />}
      </Match>
    </Switch>
  );
}

setHistoryTabOpener((target) =>
  openCustomTab({
    id: tabIdOf(target),
    title: tabTitleOf(target),
    tooltip: target.kind === "change" ? `${target.name}, ${historyDate(target.time)}` : target.path || tr("common.project"),
    icon: History,
    component: () => <LocalHistoryView target={target} />,
  }),
);
