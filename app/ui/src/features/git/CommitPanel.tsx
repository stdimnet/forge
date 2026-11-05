/**
 * Non-modal Commit tool window (Alt+0 / Ctrl+K), laid out like PHPStorm's: the changelists
 * with their files (the active one in bold; drag files between them), Unversioned Files,
 * the commit message and the Commit Options (gear) with the commit checks.
 */
import { createEffect, createMemo, createRoot, createSignal, For, on, onCleanup, Show } from "solid-js";
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Clock,
  FileDiff,
  Folder,
  ListPlus,
  LoaderCircle,
  RefreshCw,
  Settings,
  Sparkles,
  TriangleAlert,
  Undo2,
} from "lucide-solid";
import { showContextMenu, item, separator, type MenuEntry } from "@/features/contextMenu/state";
import { allConfigurations } from "@/features/run/configStore";
import { changesApi, type ChangelistView, type CommitOptions } from "./changesApi";
import {
  activeChangelist,
  changelists,
  commitSpecs,
  inclusionOf,
  includedFiles,
  setIncluded,
  setInclusion,
  setManyIncluded,
  type Inclusion,
} from "./changelists";
import {
  createPatchDialog,
  deleteChangelist,
  dropOnChangelist,
  editChangelistDialog,
  moveToChangelistDialog,
  newChangelistDialog,
  setActiveChangelist,
} from "./changesDialogs";
import { COMMIT_CHECKS, runCommitChecks } from "./commitChecks";
import { shelveDialog } from "./gitDialogs";
import { FileIcon } from "@/shell/FileIcon";
import { UiIcon } from "@/features/iconThemes/UiIcon";
import { openFile } from "@/state/editor";
import { workspaceApi } from "@/ipc/workspace";
import { textApi, type CommitMessageProblem } from "@/ipc/text";
import { getSetting, saveSetting } from "@/state/settings";
import { askChoice } from "@/features/contextMenu/dialogs";
import { openSettings } from "@/features/settings/dialogState";
import { displayStatus, gitApi, type StatusEntry } from "./api";
import { notify, repositoryChanged, runGit, status } from "./store";
import { confirmRollback, push, showDiff } from "./actions";
import { setCommitSelectionSource } from "./context";
import { openMerge, resolveConflicts } from "./conflicts";
import { generateCommitMessage } from "./aiMessage";
import { showMessageHistory } from "./extras";
import { unifiedDiff } from "./textDiff";
import { CommitSpellProblems, createCommitMessageCheck, SpellOverlay } from "@/features/spelling/commitMessage";
import { t, tOr } from "@/i18n";

// Panel state lives at module level so it survives hiding the tool window.
const [message, setMessage] = createSignal("");
const [amend, setAmend] = createSignal(false);
/** "Author:" of the Commit Options, for the next commit only. */
const [author, setAuthor] = createSignal("");
const [selected, setSelected] = createSignal<string | null>(null);
/** The changelist the selected row belongs to (or the selected changelist node). */
const [selectedList, setSelectedList] = createSignal<string | null>(null);
const [optionsOpen, setOptionsOpen] = createSignal(false);
const [progress, setProgress] = createSignal<string | null>(null);
/** Drag and drop between changelists: the list under the pointer. */
const [dropTarget, setDropTarget] = createSignal<string | null>(null);
const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
const [generating, setGenerating] = createSignal(false);
const [busy, setBusy] = createSignal(false);
const [problems, setProblems] = createSignal<CommitMessageProblem[]>([]);

export { setMessage as setCommitMessage, message as commitMessage };

// Commit message inspections (Settings | Version Control | Git), computed in Rust while typing.
createRoot(() => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const text = message();
    const rules = {
      subjectMaxLength: getSetting<number>("vcs.git.commit.subjectMaxLength", 72),
      blankLineAfterSubject: getSetting<boolean>("vcs.git.commit.blankLineAfterSubject", true),
      bodyRightMargin: getSetting<number>("vcs.git.commit.bodyRightMargin", 72),
    };
    clearTimeout(timer);
    if (!text.trim()) {
      setProblems([]);
      return;
    }
    timer = setTimeout(() => {
      void textApi
        .inspectCommitMessage(text, rules)
        .then((p) => {
          if (message() === text) setProblems(p);
        })
        .catch(() => setProblems([]));
    }, 150);
  });
});

/**
 * "Warn if CRLF line separators are about to be committed": ask before committing files
 * with CRLF separators (unless git converts them, see `git_crlf_files`).
 */
async function confirmLineSeparators(paths: string[]): Promise<boolean> {
  if (!paths.length || !getSetting<boolean>("vcs.git.commit.warnOnCrlf", true)) return true;
  const crlf = await gitApi.crlfFiles(paths).catch(() => [] as string[]);
  if (!crlf.length) return true;
  const first = crlf.slice(0, 5).join(", ");
  const list = crlf.length > 5 ? t("vcs.crlf.more", { list: first, count: crlf.length - 5 }) : first;
  const choice = await askChoice({
    title: t("vcs.crlf.title"),
    message: t("vcs.crlf.message", { list }),
    choices: [{ id: "commit", label: t("vcs.crlf.commitAsIs") }],
  });
  return choice === "commit";
}

const changes = () => (status()?.entries ?? []).filter((e) => !e.conflict && e.worktree !== "untracked" && e.worktree !== "ignored");
const conflicts = () => (status()?.entries ?? []).filter((e) => e.conflict);
const unversioned = () => (status()?.entries ?? []).filter((e) => e.worktree === "untracked");

const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const plural = (n: number) => t("vcs.files", { count: n });

function toggleCollapsed(id: string) {
  setCollapsed((prev) => {
    const next = new Set(prev);
    next.has(id) ? next.delete(id) : next.add(id);
    return next;
  });
}

/** A file of one changelist (`hunks`: its part of a partly moved file). */
interface ListItem {
  path: string;
  hunks: string[] | null;
}

function Checkbox(props: { items: ListItem[]; label: string }) {
  const state = (): Inclusion => {
    const states = props.items.map((i) => inclusionOf(i.path, i.hunks));
    if (!states.length || states.every((s) => s === "none")) return "none";
    return states.every((s) => s === "all") ? "all" : "some";
  };
  let ref!: HTMLInputElement;
  createEffect(() => (ref.indeterminate = state() === "some"));
  return (
    <input
      ref={ref}
      type="checkbox"
      class="commit-check"
      aria-label={t("a11y.commit.include", { name: props.label })}
      tabIndex={-1}
      checked={props.items.length > 0 && state() === "all"}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => setManyIncluded(props.items, e.currentTarget.checked)}
    />
  );
}

const dragType = "application/x-forge-changes";

function FileRow(props: { entry: StatusEntry; item: ListItem; list: string; depth: number; showDir: boolean }) {
  const st = () => displayStatus(props.entry);
  return (
    <div
      class="tree-row commit-row"
      classList={{ selected: selected() === props.entry.path && selectedList() === props.list }}
      style={{ "padding-left": `${8 + props.depth * 18}px` }}
      data-path={props.entry.path}
      data-changelist={props.list}
      id={commitRowId(props.list, props.entry.path)}
      role="treeitem"
      aria-level={props.depth + 1}
      aria-selected={selected() === props.entry.path && selectedList() === props.list}
      draggable={props.list !== "unversioned"}
      onDragStart={(e) => {
        e.dataTransfer?.setData(dragType, JSON.stringify({ from: props.list, items: [props.item] }));
        if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
      }}
      onClick={() => select(props.entry.path, props.list)}
      onContextMenu={(e) => {
        e.preventDefault();
        select(props.entry.path, props.list);
        showContextMenu(rowMenu(props.entry, props.item, props.list), e.clientX, e.clientY);
      }}
      onDblClick={() => (st() === "untracked" ? void openFile(props.entry.path) : void showDiff(props.entry.path, allPaths()))}
      title={props.entry.path}
    >
      <span class="tree-chevron" />
      <Checkbox items={[props.item]} label={nameOf(props.entry.path)} />
      <FileIcon name={nameOf(props.entry.path)} path={props.entry.path} />
      <span class={`tree-label vcs-${st()}`}>{nameOf(props.entry.path)}</span>
      <UiIcon id={`vcs.${st()}`} size={12} class="vcs-status-icon" />
      <Show when={props.showDir && dirOf(props.entry.path)}>
        <span class="tree-hint">{dirOf(props.entry.path)}</span>
      </Show>
      <Show when={props.item.hunks}>
        <span class="tree-hint commit-partial" title={t("vcs.partial.tooltip")}>
          {t("vcs.partial.changes", { count: props.item.hunks!.length })}
        </span>
      </Show>
    </div>
  );
}

function select(path: string | null, list: string | null) {
  setSelected(path);
  setSelectedList(list);
}

/** Element id of a changelist or file row (aria-activedescendant). */
const commitRowIds = new Map<string, string>();
function commitRowId(list: string, path: string | null): string {
  const key = `${list}\n${path ?? ""}`;
  let id = commitRowIds.get(key);
  if (!id) commitRowIds.set(key, (id = `commit-row-${commitRowIds.size + 1}`));
  return id;
}

/**
 * Keys in the changes tree: Up/Down/Home/End move between changelists and files, Left/Right
 * collapse and expand a changelist, Space includes or excludes the selection, Enter shows the
 * diff (or opens an unversioned file).
 */
function onCommitTreeKey(e: KeyboardEvent): void {
  const tree = e.currentTarget as HTMLElement;
  const rows = [...tree.querySelectorAll<HTMLElement>(".commit-group[data-changelist], .commit-row[data-path][data-changelist]")];
  const at = rows.findIndex((r) =>
    selected() ? r.dataset.path === selected() && r.dataset.changelist === selectedList() : !r.dataset.path && r.dataset.changelist === selectedList(),
  );
  const pick = (r: HTMLElement | undefined) => {
    if (!r) return;
    select(r.dataset.path ?? null, r.dataset.changelist ?? null);
    r.scrollIntoView?.({ block: "nearest" });
  };
  const row = rows[at];
  switch (e.key) {
    case "ArrowDown":
      pick(rows[at < 0 ? 0 : Math.min(rows.length - 1, at + 1)]);
      break;
    case "ArrowUp":
      pick(rows[at < 0 ? 0 : Math.max(0, at - 1)]);
      break;
    case "Home":
      pick(rows[0]);
      break;
    case "End":
      pick(rows[rows.length - 1]);
      break;
    case "ArrowLeft":
    case "ArrowRight": {
      if (!row || row.dataset.path) return;
      const id = row.dataset.changelist!;
      if (collapsed().has(id) === (e.key === "ArrowRight")) toggleCollapsed(id);
      break;
    }
    case " ":
      row?.querySelector<HTMLInputElement>(".commit-check")?.click();
      break;
    case "Enter": {
      const path = row?.dataset.path;
      if (!path) return;
      if (row.dataset.changelist === "unversioned") void openFile(path);
      else void showDiff(path, allPaths());
      break;
    }
    default:
      return;
  }
  e.preventDefault();
  e.stopPropagation();
}

/** One changelist node ("Changes", user lists, "Unversioned Files") with files grouped by directory. */
function Group(props: { id: string; title: string; items: ListItem[]; strong?: boolean; list?: ChangelistView; showEmpty?: boolean }) {
  const open = () => !collapsed().has(props.id);
  const byPath = () => new Map((status()?.entries ?? []).map((e) => [e.path, e]));
  const entries = createMemo(() =>
    props.items
      .map((i) => ({ item: i, entry: byPath().get(i.path) }))
      .filter((x): x is { item: ListItem; entry: StatusEntry } => !!x.entry)
      .sort((a, b) => a.item.path.localeCompare(b.item.path)),
  );
  const dirs = createMemo(() => {
    const map = new Map<string, { item: ListItem; entry: StatusEntry }[]>();
    for (const e of entries()) {
      const d = dirOf(e.item.path);
      map.set(d, [...(map.get(d) ?? []), e]);
    }
    return [...map.entries()];
  });
  const droppable = () => !!props.list;
  return (
    <Show when={entries().length > 0 || props.showEmpty}>
      <div
        class="tree-row commit-group"
        classList={{ selected: !selected() && selectedList() === props.id, "drop-target": dropTarget() === props.id }}
        data-changelist={props.id}
        id={commitRowId(props.id, null)}
        role="treeitem"
        aria-level={1}
        aria-expanded={open()}
        aria-selected={!selected() && selectedList() === props.id}
        onClick={() => {
          select(null, props.id);
          toggleCollapsed(props.id);
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          select(null, props.id);
          if (props.list) showContextMenu(listMenu(props.list), e.clientX, e.clientY);
        }}
        onDblClick={() => props.list && editChangelistDialog(props.list)}
        onDragOver={(e) => {
          if (!droppable() || !e.dataTransfer?.types.includes(dragType)) return;
          e.preventDefault();
          setDropTarget(props.id);
        }}
        onDragLeave={() => dropTarget() === props.id && setDropTarget(null)}
        onDrop={(e) => {
          setDropTarget(null);
          const data = e.dataTransfer?.getData(dragType);
          if (!droppable() || !data) return;
          e.preventDefault();
          const { from, items } = JSON.parse(data) as { from: string; items: ListItem[] };
          if (from !== props.id) dropOnChangelist(props.id, items);
        }}
        style={{ "padding-left": "8px" }}
        title={props.list?.comment || undefined}
      >
        <span class="tree-chevron">{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>
        <Checkbox items={entries().map((e) => e.item)} label={props.title} />
        <span class="tree-label" classList={{ strong: props.strong }}>
          {props.title}
        </span>
        <span class="tree-hint">{entries().length ? plural(entries().length) : "empty"}</span>
      </div>
      <Show when={open()}>
        <For each={dirs()}>
          {([dir, files]) => (
            <Show
              when={dir && files.length > 1}
              fallback={<For each={files}>{(e) => <FileRow entry={e.entry} item={e.item} list={props.id} depth={1} showDir />}</For>}
            >
              <div
                class="tree-row"
                style={{ "padding-left": "26px" }}
                role="treeitem"
                aria-level={2}
                aria-expanded={!collapsed().has(`${props.id}:${dir}`)}
                aria-selected="false"
                onClick={() => toggleCollapsed(`${props.id}:${dir}`)}
              >
                <span class="tree-chevron">
                  {collapsed().has(`${props.id}:${dir}`) ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
                </span>
                <Checkbox items={files.map((e) => e.item)} label={dir} />
                <Folder size={15} stroke-width={1.75} class="muted" />
                <span class="tree-label">{dir}</span>
                <span class="tree-hint">{plural(files.length)}</span>
              </div>
              <Show when={!collapsed().has(`${props.id}:${dir}`)}>
                <For each={files}>{(e) => <FileRow entry={e.entry} item={e.item} list={props.id} depth={2} showDir={false} />}</For>
              </Show>
            </Show>
          )}
        </For>
      </Show>
    </Show>
  );
}

/** The context menu of a changelist node. */
function listMenu(list: ChangelistView): MenuEntry[] {
  const paths = list.files.map((f) => f.path);
  return [
    item(t("git.commitPanel.commit"), () => commitList(list), "CheckinProject"),
    item(t("git.commitPanel.rollback"), () => confirmRollback(paths), "ChangesView.Rollback", { disabled: !paths.length }),
    separator,
    item(t("git.commitPanel.newChangelist"), () => newChangelistDialog(), "ChangesView.NewChangeList"),
    item(t("git.commitPanel.editChangelist"), () => editChangelistDialog(list), "ChangesView.Rename"),
    item(t("git.commitPanel.deleteChangelist"), () => void deleteChangelist(list), "ChangesView.RemoveChangeList", { disabled: list.active }),
    item(t("git.commitPanel.setActiveChangelist"), () => void setActiveChangelist(list), "ChangesView.SetDefault", { disabled: list.active }),
    separator,
    item(t("git.commitPanel.shelveChanges"), () => shelveDialog(paths), "ChangesView.Shelve", { disabled: !paths.length }),
    item(t("git.commitPanel.createPatch"), () => createPatchDialog(paths, list.name), "ChangesView.CreatePatch", { disabled: !paths.length }),
  ];
}

/** The context menu of a file row. */
function rowMenu(entry: StatusEntry, file: ListItem, list: string): MenuEntry[] {
  const tracked = entry.worktree !== "untracked";
  const target = [entry.path];
  return [
    item(t("git.commitPanel.commitFile"), () => checkOnly(target), "CheckinFiles"),
    item(t("git.commitPanel.rollback"), () => confirmRollback(target), "ChangesView.Rollback", { disabled: !tracked }),
    item(t("git.commitPanel.showDiff"), () => void showDiff(entry.path, allPaths()), "Compare.SameVersion"),
    item(t("git.commitPanel.jumpToSource"), () => void openFile(entry.path), "EditSource"),
    separator,
    item(
      "Move to Another Changelist…",
      () => moveToChangelistDialog(target, { hunks: file.hunks ?? undefined, from: list }),
      "ChangesView.Move",
      { disabled: !tracked },
    ),
    item(t("git.commitPanel.newChangelist"), () => newChangelistDialog(tracked ? target : []), "ChangesView.NewChangeList"),
    separator,
    item(t("git.commitPanel.shelveChanges"), () => shelveDialog(target), "ChangesView.Shelve", { disabled: !tracked }),
    item(t("git.commitPanel.createPatch"), () => createPatchDialog(target, nameOf(entry.path)), "ChangesView.CreatePatch"),
  ];
}

/** Commit… on a changelist node: include exactly its files. */
function commitList(list: ChangelistView) {
  setIncluded([]);
  setManyIncluded(list.files, true);
  document.querySelector<HTMLTextAreaElement>(".commit-message textarea")?.focus();
}

/** The "Merge Conflicts" node: unresolved files in red with a Resolve link, like PHPStorm. */
function ConflictsGroup() {
  const open = () => !collapsed().has("conflicts");
  return (
    <Show when={conflicts().length > 0}>
      <div
        class="tree-row commit-group commit-conflicts"
        role="treeitem"
        aria-level={1}
        aria-expanded={open()}
        aria-selected="false"
        onClick={() => toggleCollapsed("conflicts")}
        style={{ "padding-left": "8px" }}
      >
        <span class="tree-chevron">{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>
        <span class="tree-label strong">{t("vcs.mergeConflicts")}</span>
        <span class="tree-hint">{plural(conflicts().length)}</span>
        <button
          class="link-btn commit-resolve"
          onClick={(e) => {
            e.stopPropagation();
            resolveConflicts();
          }}
        >
          {t("git.commitPanel.resolve")}
        </button>
      </div>
      <Show when={open()}>
        <For each={[...conflicts()].sort((a, b) => a.path.localeCompare(b.path))}>
          {(e) => (
            <div
              class="tree-row commit-row"
              classList={{ selected: selected() === e.path }}
              style={{ "padding-left": "26px" }}
              data-path={e.path}
              role="treeitem"
              aria-level={2}
              aria-selected={selected() === e.path}
              onClick={() => setSelected(e.path)}
              onDblClick={() => void openMerge(e.path)}
              title={e.path}
            >
              <span class="tree-chevron" />
              <FileIcon name={nameOf(e.path)} />
              <span class="tree-label vcs-conflict">{nameOf(e.path)}</span>
              <Show when={dirOf(e.path)}>
                <span class="tree-hint">{dirOf(e.path)}</span>
              </Show>
            </div>
          )}
        </For>
      </Show>
    </Show>
  );
}

const allPaths = () => [...changes(), ...unversioned()].map((e) => e.path);
const checkedPaths = () => allPaths().filter((p) => includedFiles().has(p));

async function generate() {
  const paths = checkedPaths();
  if (!paths.length) {
    notify("info", t("git.commitPanel.selectFilesToGenerateA"));
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
    notify("error", t("git.commitPanel.couldNotGenerateCommitMessage"), String(err instanceof Error ? err.message : err), 10000);
  } finally {
    setGenerating(false);
  }
}

/** The options of the next commit (Commit Options and the checkboxes below the message). */
function commitOptions(): CommitOptions {
  return {
    amend: amend(),
    signOff: getSetting<boolean>("vcs.git.commit.signOff", false),
    noVerify: !getSetting<boolean>("vcs.commit.runHooks", true),
    author: author().trim() || null,
  };
}

async function doCommit(pushAfter: boolean) {
  if (busy()) return;
  setBusy(true);
  setOptionsOpen(false);
  try {
    const paths = checkedPaths();
    if (!(await confirmLineSeparators(paths))) return;
    if (!(await runCommitChecks(paths, setProgress))) return;
    setProgress("Committing…");
    const specs = await commitSpecs(paths);
    if (!specs.length && !amend()) return notify("info", t("git.commitPanel.noChangesAreIncludedIn"));
    const msg = message();
    const sha = await runGit(t("git.commitPanel.commitFailed"), () => changesApi.commitFiles(msg, specs, commitOptions()));
    if (!sha) return;
    const partial = specs.filter((s) => s.hunks).length;
    notify(
      "success",
      t("git.commitPanel.committed", { count: specs.length, partial, subject: msg.split("\n")[0] }),
    );
    // What is left of partly committed files is not included any more.
    for (const s of specs) if (s.hunks) setInclusion(s.path, null, false);
    setMessage("");
    setAmend(false);
    setAuthor("");
    if (pushAfter) await push();
  } finally {
    setProgress(null);
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

/** The changelists with their files; changes no list knows yet show in the active list. */
const listGroups = createMemo(() => {
  const changed = new Set(changes().map((e) => e.path));
  const lists = changelists();
  const seen = new Set<string>();
  const groups = lists.map((l) => {
    const items = l.files.filter((f) => changed.has(f.path));
    items.forEach((f) => seen.add(f.path));
    return { list: l as ChangelistView | undefined, id: l.id, title: l.name, active: l.active, items: items as ListItem[] };
  });
  const rest: ListItem[] = [...changed].filter((p) => !seen.has(p)).map((path) => ({ path, hunks: null }));
  const target = groups.find((g) => g.active) ?? groups[0];
  if (target) target.items = [...target.items, ...rest];
  else groups.push({ list: undefined, id: "changes", title: t("vcs.changes"), active: true, items: rest });
  return groups;
});

// The active changelist's comment is the commit message draft, like PHPStorm.
let lastComment = "";
createRoot(() =>
  createEffect(
    on(
      () => activeChangelist()?.comment ?? "",
      (comment) => {
        if (!message().trim() || message() === lastComment) setMessage(comment);
        lastComment = comment;
      },
    ),
  ),
);

/** Commit Options (the gear): Git options and the commit checks, saved as settings. */
function CommitOptionsPopup() {
  let ref!: HTMLDivElement;
  const close = (e: MouseEvent) => {
    if (!ref.contains(e.target as Node) && !(e.target as HTMLElement).closest(".commit-options-btn")) setOptionsOpen(false);
  };
  document.addEventListener("mousedown", close, true);
  onCleanup(() => document.removeEventListener("mousedown", close, true));
  const setting = (key: string, fallback = false) => getSetting<boolean>(key, fallback);
  const toggle = (key: string, value: boolean) => void saveSetting(key, value);
  const configs = () => allConfigurations();
  return (
    <div class="commit-options-popup" ref={ref} role="dialog" aria-label={t("vcs.commitOptions")}>
      <div class="commit-options-section">Git</div>
      <label class="commit-options-row commit-options-author">
        {t("vcs.author")}
        <input class="gd-input" value={author()} placeholder={t("vcs.authorPlaceholder")} spellcheck={false} onInput={(e) => setAuthor(e.currentTarget.value)} />
      </label>
      <label class="commit-options-row">
        <input type="checkbox" checked={setting("vcs.git.commit.signOff")} onChange={(e) => toggle("vcs.git.commit.signOff", e.currentTarget.checked)} />
        {t("vcs.signOffCommit")}
      </label>
      <label class="commit-options-row">
        <input type="checkbox" checked={amend()} onChange={(e) => void toggleAmend(e.currentTarget.checked)} />
        {t("vcs.amendCommit")}
      </label>
      <label class="commit-options-row">
        <input type="checkbox" checked={setting("vcs.commit.runHooks", true)} onChange={(e) => toggle("vcs.commit.runHooks", e.currentTarget.checked)} />
        {t("vcs.runHooks")}
      </label>
      <div class="commit-options-section">{t("vcs.commitChecks")}</div>
      <For each={COMMIT_CHECKS}>
        {(c) => (
          <label class="commit-options-row">
            <input type="checkbox" checked={setting(c.key)} onChange={(e) => toggle(c.key, e.currentTarget.checked)} />
            {tOr(`vcs.commitCheck.${c.key}`, c.label)}
          </label>
        )}
      </For>
      <div class="commit-options-section">{t("vcs.advancedChecks")}</div>
      <label class="commit-options-row">
        <input type="checkbox" checked={setting("vcs.commit.runTests")} onChange={(e) => toggle("vcs.commit.runTests", e.currentTarget.checked)} />
        {t("vcs.runTests")}
        <select
          class="gd-select"
          disabled={!setting("vcs.commit.runTests")}
          value={getSetting<string>("vcs.commit.testConfiguration", "")}
          onChange={(e) => void saveSetting("vcs.commit.testConfiguration", e.currentTarget.value)}
        >
          <option value="">{t("vcs.chooseConfiguration")}</option>
          <For each={configs()}>{(c) => <option value={c.name}>{c.name}</option>}</For>
        </select>
      </label>
      <button class="link-btn commit-options-settings" onClick={() => (setOptionsOpen(false), openSettings("vcs.commit"))}>
        {t("vcs.commitSettingsLink")}
      </button>
    </div>
  );
}

export function CommitPanel() {
  const canCommit = () => !busy() && (checkedPaths().length > 0 || amend()) && message().trim().length > 0;
  const total = () => conflicts().length + changes().length + unversioned().length;
  let messageInput: HTMLTextAreaElement | undefined;
  // Spelling and grammar of the message (Settings | Editor | Natural Languages).
  const spellProblems = createCommitMessageCheck(message);
  // Ctrl+D comes through the keymap (CompareTwoFiles → the Commit window's compare handler).
  return (
    <div class="commit-panel" tabIndex={-1}>
      <div class="panel-toolbar">
        <button class="icon-btn" title={t("vcs.refresh", { shortcut: "Ctrl+F5" })} onClick={() => void repositoryChanged()}>
          <RefreshCw size={15} />
        </button>
        <button
          class="icon-btn"
          title={t("vcs.rollback", { shortcut: "Ctrl+Alt+Z" })}
          disabled={!checkedPaths().length}
          onClick={() => confirmRollback(checkedPaths().filter((p) => changes().some((c) => c.path === p)))}
        >
          <Undo2 size={15} />
        </button>
        <button
          class="icon-btn"
          title={t("vcs.showDiff", { shortcut: "Ctrl+D" })}
          disabled={!selected() && !checkedPaths().length}
          onClick={() => void showDiff(selected() ?? checkedPaths()[0], allPaths())}
        >
          <FileDiff size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title={t("common.expandAll", { shortcut: "Ctrl+NumPad +" })} onClick={() => setCollapsed(new Set<string>())}>
          <ChevronsUpDown size={15} />
        </button>
        <button
          class="icon-btn"
          title={t("common.collapseAll", { shortcut: "Ctrl+NumPad -" })}
          onClick={() => setCollapsed(new Set(["conflicts", "unversioned", ...listGroups().map((g) => g.id)]))}
        >
          <ChevronsDownUp size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title={t("vcs.newChangelist")} onClick={() => newChangelistDialog()}>
          <ListPlus size={15} />
        </button>
      </div>
      <div
        class="commit-tree"
        role={total() > 0 ? "tree" : undefined}
        aria-label={t("toolwindow.commit.title")}
        aria-multiselectable="true"
        tabIndex={0}
        aria-activedescendant={selected() ? commitRowId(selectedList() ?? "", selected()) : selectedList() ? commitRowId(selectedList()!, null) : undefined}
        onKeyDown={onCommitTreeKey}
      >
        <Show
          when={total() > 0}
          fallback={<div class="commit-empty">{status() ? t("vcs.noChanges") : t("vcs.noRepository")}</div>}
        >
          <ConflictsGroup />
          <For each={listGroups()}>
            {(g) => <Group id={g.id} title={g.title} items={g.items} strong={g.active} list={g.list} showEmpty={listGroups().length > 1} />}
          </For>
          <Group id="unversioned" title={t("vcs.unversioned")} items={unversioned().map((e) => ({ path: e.path, hunks: null }))} />
        </Show>
      </div>
      <div class="commit-message">
        <div class="commit-message-toolbar">
          <Show when={status()?.branch}>
            <span class="commit-branch-hint" title={t("vcs.currentBranch")}>
              {status()!.branch}
              <Show when={(status()?.ahead ?? 0) > 0}>
                <span class="muted"> ↑{status()!.ahead}</span>
              </Show>
            </span>
          </Show>
          <span class="spacer" />
          <button class="icon-btn" title={t("vcs.messageHistory", { shortcut: "Ctrl+M" })} onClick={() => void showMessageHistory()} data-testid="message-history-btn">
            <Clock size={14} />
          </button>
          <button
            class="icon-btn ai-generate"
            classList={{ busy: generating() }}
            title={t("vcs.generateMessage")}
            disabled={generating()}
            onClick={() => void generate()}
          >
            <Show when={generating()} fallback={<Sparkles size={15} />}>
              <LoaderCircle size={15} class="spin" />
            </Show>
          </button>
        </div>
        <div class="spell-textarea">
          <textarea
            ref={messageInput}
            placeholder={t("vcs.commitMessage")}
            aria-label={t("vcs.commitMessage")}
            data-focus-context="toolwindow-input"
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
          <SpellOverlay text={message()} problems={spellProblems()} textarea={() => messageInput} />
        </div>
        <Show when={problems().length || spellProblems().length}>
          <ul class="commit-problems" data-commit-problems>
            <For each={problems()}>
              {(p) => (
                <li title={t("vcs.inspectionsLink")} onClick={() => openSettings("vcs.git")}>
                  <TriangleAlert size={12} />
                  <span>
                    {t("vcs.problemLine", { line: p.line + 1, message: p.message })}
                  </span>
                </li>
              )}
            </For>
            <CommitSpellProblems
              text={message()}
              problems={spellProblems()}
              onReplace={(r) => setMessage((m) => m.slice(0, r.from) + r.text + m.slice(r.to))}
            />
          </ul>
        </Show>
        <div class="commit-options">
          <label class="commit-amend">
            <input type="checkbox" checked={amend()} onChange={(e) => void toggleAmend(e.currentTarget.checked)} />
            {t("vcs.amend")}
          </label>
          <label class="commit-amend" title={t("vcs.signOff.tooltip")}>
            <input
              type="checkbox"
              checked={getSetting<boolean>("vcs.git.commit.signOff", false)}
              onChange={(e) => void saveSetting("vcs.git.commit.signOff", e.currentTarget.checked)}
            />
            {t("vcs.signOff")}
          </label>
          <Show when={generating()}>
            <span class="muted ai-status">{t("vcs.generating")}</span>
          </Show>
          <Show when={progress()}>
            <span class="muted ai-status commit-progress">
              <LoaderCircle size={13} class="spin" /> {progress()}
            </span>
          </Show>
        </div>
        <div class="commit-actions">
          <button class="btn-primary" disabled={!canCommit()} onClick={() => void doCommit(false)} title={t("vcs.commit.tooltip", { shortcut: "Ctrl+Enter" })}>
            {amend() ? t("vcs.amendCommitButton") : t("vcs.commit")}
          </button>
          <button class="btn-secondary" disabled={!canCommit()} onClick={() => void doCommit(true)} title={t("vcs.commitAndPush.tooltip", { shortcut: "Ctrl+Alt+K" })}>
            {amend() ? t("vcs.amendCommitAndPush") : t("vcs.commitAndPush")}
          </button>
          <span class="spacer" />
          <button class="icon-btn commit-options-btn" classList={{ active: optionsOpen() }} title={t("vcs.commitOptions")} onClick={() => setOptionsOpen(!optionsOpen())}>
            <Settings size={15} />
          </button>
          <Show when={optionsOpen()}>
            <CommitOptionsPopup />
          </Show>
        </div>
      </div>
    </div>
  );
}

/** Select a file in the commit tree (used by screenshots/commands). */
export function selectCommitFile(path: string): void {
  setSelected(path);
}

/** "Commit File…" / "Commit Directory…": include exactly these files in the next commit. */
export function checkOnly(paths: string[]): void {
  setIncluded(paths);
  if (paths.length) select(paths[0], null);
}

/** Move to Another Changelist (F6) for the Commit window's selection. */
export function moveSelectionToChangelist(): boolean {
  const path = selected();
  const list = selectedList();
  if (!path) return false;
  const group = listGroups().find((g) => g.id === list);
  const item = group?.items.find((i) => i.path === path);
  moveToChangelistDialog([path], { hunks: item?.hunks ?? undefined, from: list ?? undefined });
  return true;
}

setCommitSelectionSource(selected, allPaths);
