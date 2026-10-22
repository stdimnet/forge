/**
 * History tabs of the Git tool window: "History: <file>" (Show History of a file or folder)
 * and "History for Selection: <file>" (Show History for Selection). A commit list, the
 * commit details and a diff preview of the selected revision; the revision actions are in
 * the row context menu and the toolbar.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, Show } from "solid-js";
import { Columns2, Copy, Download, FileDiff, History, MessageSquareQuote, RefreshCw, X } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { workspace } from "@/state/workspace";
import { item, separator, showContextMenu } from "@/features/contextMenu/state";
import { registerCompareHandler } from "@/features/compare/state";
import { gitApi, type ChangedFile } from "./api";
import { historyApi, type BlockRevision, type FileRevision } from "./historyApi";
import { revision } from "./store";
import { fullDate, logDate } from "./format";
import { InlineDiff } from "./InlineDiff";
import "./history.css";
import {
  activeGitTab,
  annotateRevision,
  checkoutRevision,
  closeHistoryTab,
  compareRevisionWithLocal,
  copyRevisionNumber,
  getRevision,
  historyTabs,
  historyTabTitle,
  revisionTexts,
  setActiveGitTab,
  shortSha,
  showRevisionChange,
  type HistoryTab,
  type RevisionFile,
} from "./history";

const PAGE = 200;

/** Show Diff of the history tab that last had the focus (Ctrl+D). */
let focusedShowDiff: (() => void) | null = null;
registerCompareHandler(".file-history", () => focusedShowDiff?.());

/** The tab strip of the Git tool window: Log, the history tabs, Console. */
export function GitTabs() {
  return (
    <div class="git-tabs" role="tablist">
      <span class="git-tab" role="tab" classList={{ active: activeGitTab() === "log" }} onClick={() => setActiveGitTab("log")}>
        Log
      </span>
      <For each={historyTabs()}>
        {(tab) => (
          <span
            class="git-tab git-history-tab"
            role="tab"
            classList={{ active: activeGitTab() === tab.id }}
            title={tab.path || workspace()?.name}
            onClick={() => setActiveGitTab(tab.id)}
            onMouseDown={(e) => e.button === 1 && closeHistoryTab(tab.id)}
          >
            <History size={12} class="muted" />
            {historyTabTitle(tab, workspace()?.name)}
            <button
              class="git-tab-close"
              title="Close Tab"
              onClick={(e) => {
                e.stopPropagation();
                closeHistoryTab(tab.id);
              }}
            >
              <X size={12} />
            </button>
          </span>
        )}
      </For>
      <span class="git-tab">Console</span>
    </div>
  );
}

/** The history tabs' content; only the active one is visible, the others keep their state. */
export function HistoryTabs() {
  return (
    <For each={historyTabs()}>
      {(tab) => (
        <div class="git-history-body" classList={{ hidden: activeGitTab() !== tab.id }}>
          <Show when={tab.kind === "selection"} fallback={<FileHistoryView tab={tab} />}>
            <SelectionHistoryView tab={tab} />
          </Show>
        </div>
      )}
    </For>
  );
}

/** Up/Down in a revision list. */
function listKeys(ids: () => string[], selected: () => string | null, select: (id: string) => void, root: () => HTMLElement) {
  return (e: KeyboardEvent) => {
    const list = ids();
    const i = list.indexOf(selected() ?? "");
    let next: string | undefined;
    if (e.key === "ArrowDown") next = list[Math.min(list.length - 1, i + 1)];
    else if (e.key === "ArrowUp") next = list[Math.max(0, i - 1)];
    else return;
    e.preventDefault();
    if (next) {
      select(next);
      root().querySelector(`[data-sha="${next}"]`)?.scrollIntoView({ block: "nearest" });
    }
  };
}

function RevisionRow(props: {
  rev: { sha: string; subject: string; author: string; time: number };
  selected: boolean;
  /** Path of the file in this revision when it differs from the tab's (renames). */
  renamed?: string;
  onSelect: () => void;
  onMenu: (e: MouseEvent) => void;
  onOpen: () => void;
}) {
  return (
    <div
      class="history-row"
      classList={{ selected: props.selected }}
      data-sha={props.rev.sha}
      onClick={props.onSelect}
      onDblClick={props.onOpen}
      onContextMenu={(e) => {
        e.preventDefault();
        props.onSelect();
        props.onMenu(e);
      }}
    >
      <div class="history-subject">
        <span class="log-subject-text">{props.rev.subject}</span>
        <Show when={props.renamed}>
          <span class="history-renamed" title={`Path in this revision: ${props.renamed}`}>
            {props.renamed}
          </span>
        </Show>
      </div>
      <div class="log-author">{props.rev.author}</div>
      <div class="log-date">{logDate(props.rev.time)}</div>
      <div class="history-hash">{shortSha(props.rev.sha)}</div>
    </div>
  );
}

function CommitMessage(props: { sha: string | null }) {
  const [details] = createResource(
    () => props.sha,
    (sha) => gitApi.commitDetails(sha),
  );
  return (
    <Show when={details()} keyed fallback={<div class="commit-empty">{props.sha ? "Loading…" : "Select a revision"}</div>}>
      {(detail) => {
        const d = () => detail;
        const [subject, ...body] = detail.message.split("\n");
        return (
          <div class="log-message">
            <div class="log-subject">{subject}</div>
            <Show when={body.join("\n").trim()}>
              <p class="log-body">{body.join("\n").trim().replace(/([^\n])\n(?!\n)/g, "$1 ")}</p>
            </Show>
            <div class="log-meta">
              <span class="log-sha">{shortSha(d().sha)}</span> {d().author} <span class="muted">&lt;{d().email}&gt;</span> on{" "}
              {fullDate(d().time)}
            </div>
          </div>
        );
      }}
    </Show>
  );
}

function FileHistoryView(props: { tab: HistoryTab }) {
  const [revs, setRevs] = createSignal<FileRevision[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [exhausted, setExhausted] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [selectedSha, setSelectedSha] = createSignal<string | null>(null);
  /** Folder history: the file of the selected commit shown in the diff preview. */
  const [selectedFile, setSelectedFile] = createSignal<string | null>(null);
  let list!: HTMLDivElement;

  const load = async (reset: boolean) => {
    if (loading() || (!reset && exhausted())) return;
    setLoading(true);
    try {
      const page = await historyApi.fileHistory(props.tab.path, props.tab.dir, { skip: reset ? 0 : revs().length, limit: PAGE });
      setError(null);
      setExhausted(page.length < PAGE);
      setRevs(reset ? page : [...revs(), ...page]);
      if (reset && !page.some((r) => r.sha === selectedSha())) setSelectedSha(page[0]?.sha ?? null);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
      setRevs([]);
    } finally {
      setLoading(false);
    }
  };
  createEffect(on(revision, () => void load(true)));

  const current = () => revs().find((r) => r.sha === selectedSha());
  const files = (): ChangedFile[] => current()?.files ?? [];
  const file = (): ChangedFile | undefined => files().find((f) => f.path === selectedFile()) ?? files()[0];
  const revFile = (): RevisionFile | null => {
    const r = current();
    const f = file();
    return r && f ? { sha: r.sha, parent: r.parents[0], file: f } : null;
  };
  /** The working-copy path of the selected file (the tab's file, or the file in the folder). */
  const localPath = () => (props.tab.dir ? (file()?.path ?? null) : props.tab.path);

  const [texts] = createResource(revFile, (r) => revisionTexts(r));

  const showDiff = () => {
    const r = revFile();
    if (r) void showRevisionChange(r);
  };
  const withLocal = () => {
    const r = revFile();
    const local = localPath();
    if (r && local) void compareRevisionWithLocal(r, local);
  };
  const get = () => {
    const r = revFile();
    const local = localPath();
    if (r && local) void getRevision(r, local);
  };
  const annotate = () => {
    const r = revFile();
    if (r) void annotateRevision(r);
  };

  const menu = (e: MouseEvent) => {
    const r = current();
    if (!r) return;
    showContextMenu(
      [
        item("Show Diff", showDiff, "Compare.SameVersion", { shortcutAction: "CompareTwoFiles", disabled: !revFile() }),
        item("Compare with Local", withLocal, undefined, { disabled: !revFile() }),
        separator,
        item("Get", get, undefined, { disabled: !revFile() || file()?.status === "D" }),
        item("Annotate Revision", annotate, undefined, { disabled: !revFile() }),
        item("Checkout Revision", () => void checkoutRevision(r.sha)),
        separator,
        item("Copy Revision Number", () => void copyRevisionNumber(r.sha), "Vcs.CopyRevisionNumberAction"),
      ],
      e.clientX,
      e.clientY,
    );
  };

  const leftTitle = () => {
    const r = revFile();
    if (!r) return "";
    if (r.file.status === "A" || !r.parent) return "Empty (file added)";
    const from = r.file.origPath ?? r.file.path;
    return `${shortSha(r.parent)}${from !== r.file.path ? ` (${from})` : ""}`;
  };

  return (
    <div class="file-history" onFocusIn={() => (focusedShowDiff = showDiff)}>
      <div class="file-history-main">
        <div class="panel-toolbar history-toolbar">
          <button class="icon-btn" title="Refresh" onClick={() => void load(true)}>
            <RefreshCw size={15} />
          </button>
          <button class="icon-btn" title="Show Diff (Ctrl+D)" disabled={!revFile()} onClick={showDiff}>
            <Columns2 size={15} />
          </button>
          <button class="icon-btn" title="Compare with Local" disabled={!revFile()} onClick={withLocal}>
            <FileDiff size={15} />
          </button>
          <button class="icon-btn" title="Get" disabled={!revFile() || file()?.status === "D"} onClick={get}>
            <Download size={15} />
          </button>
          <button class="icon-btn" title="Annotate Revision" disabled={!revFile()} onClick={annotate}>
            <MessageSquareQuote size={15} />
          </button>
          <button class="icon-btn" title="Copy Revision Number" disabled={!current()} onClick={() => current() && void copyRevisionNumber(current()!.sha)}>
            <Copy size={15} />
          </button>
          <span class="spacer" />
          <span class="history-path" title={props.tab.path}>
            {props.tab.path || workspace()?.name}
          </span>
        </div>
        <div class="history-columns">
          <div>Commit</div>
          <div>Author</div>
          <div>Date</div>
          <div>Hash</div>
        </div>
        <div
          class="log-list history-list"
          tabIndex={0}
          ref={list}
          onKeyDown={listKeys(
            () => revs().map((r) => r.sha),
            selectedSha,
            (sha) => {
              setSelectedSha(sha);
              setSelectedFile(null);
            },
            () => list,
          )}
          onScroll={(e) => {
            const el = e.currentTarget;
            if (el.scrollTop + el.clientHeight > el.scrollHeight - 200) void load(false);
          }}
        >
          <For each={revs()}>
            {(r) => (
              <RevisionRow
                rev={r}
                selected={selectedSha() === r.sha}
                renamed={!props.tab.dir && r.files[0] && r.files[0].path !== props.tab.path ? r.files[0].path : undefined}
                onSelect={() => {
                  setSelectedSha(r.sha);
                  setSelectedFile(null);
                }}
                onMenu={menu}
                onOpen={showDiff}
              />
            )}
          </For>
          <Show when={!revs().length && !loading()}>
            <div class="commit-empty">{error() ?? "No history: the file is not committed yet"}</div>
          </Show>
        </div>
        <div class="history-details">
          <Show when={props.tab.dir && files().length}>
            <div class="history-files">
              <For each={files()}>
                {(f) => (
                  <div
                    class="tree-row"
                    classList={{ selected: file()?.path === f.path }}
                    style={{ "padding-left": "10px" }}
                    onClick={() => setSelectedFile(f.path)}
                    onDblClick={() => {
                      setSelectedFile(f.path);
                      showDiff();
                    }}
                  >
                    <FileIcon name={f.path.split("/").pop()!} />
                    <span class={`tree-label ${f.status === "A" ? "vcs-added" : f.status === "D" ? "vcs-deleted" : "vcs-modified"}`}>
                      {f.path.split("/").pop()}
                    </span>
                    <span class="tree-hint">{f.path.split("/").slice(0, -1).join("/")}</span>
                  </div>
                )}
              </For>
            </div>
          </Show>
          <CommitMessage sha={selectedSha()} />
        </div>
      </div>
      <Show
        when={revFile()}
        fallback={
          <div class="inline-diff">
            <div class="commit-empty">Select a revision to view the changes</div>
          </div>
        }
      >
        {(r) => (
          <InlineDiff
            path={r().file.path}
            left={{ title: leftTitle(), content: texts()?.before ?? "" }}
            right={{ title: r().file.status === "D" ? `${shortSha(r().sha)} (file deleted)` : `${shortSha(r().sha)} · ${r().file.path}`, content: texts()?.after ?? "" }}
          />
        )}
      </Show>
    </div>
  );
}

function SelectionHistoryView(props: { tab: HistoryTab }) {
  const [revs, { refetch }] = createResource(
    () => props.tab,
    (t) => historyApi.blockHistory(t.path, t.start ?? 1, t.end ?? t.start ?? 1),
  );
  const [selectedSha, setSelectedSha] = createSignal<string | null>(null);
  let list!: HTMLDivElement;
  createEffect(on(revision, () => void refetch(), { defer: true }));
  createEffect(() => {
    const list = revs() ?? [];
    if (!list.some((r) => r.sha === selectedSha())) setSelectedSha(list[0]?.sha ?? null);
  });
  const current = createMemo((): BlockRevision | undefined => revs()?.find((r) => r.sha === selectedSha()));
  const revFile = (r: BlockRevision): RevisionFile => ({
    sha: r.sha,
    parent: r.parents[0],
    file: { path: r.path, origPath: null, status: r.before === null ? "A" : "M" },
  });
  const showDiff = () => {
    const r = current();
    if (r) void showRevisionChange(revFile(r));
  };
  const menu = (e: MouseEvent) => {
    const r = current();
    if (!r) return;
    showContextMenu(
      [
        item("Show Diff", showDiff, "Compare.SameVersion", { shortcutAction: "CompareTwoFiles" }),
        item("Compare with Local", () => void compareRevisionWithLocal(revFile(r), props.tab.path)),
        separator,
        item("Annotate Revision", () => void annotateRevision(revFile(r))),
        item("Checkout Revision", () => void checkoutRevision(r.sha)),
        separator,
        item("Copy Revision Number", () => void copyRevisionNumber(r.sha), "Vcs.CopyRevisionNumberAction"),
      ],
      e.clientX,
      e.clientY,
    );
  };
  const lines = () => (props.tab.start === props.tab.end ? `line ${props.tab.start}` : `lines ${props.tab.start}–${props.tab.end}`);

  return (
    <div class="file-history" onFocusIn={() => (focusedShowDiff = showDiff)}>
      <div class="file-history-main">
        <div class="panel-toolbar history-toolbar">
          <button class="icon-btn" title="Refresh" onClick={() => void refetch()}>
            <RefreshCw size={15} />
          </button>
          <button class="icon-btn" title="Show Diff (Ctrl+D)" disabled={!current()} onClick={showDiff}>
            <Columns2 size={15} />
          </button>
          <button class="icon-btn" title="Copy Revision Number" disabled={!current()} onClick={() => current() && void copyRevisionNumber(current()!.sha)}>
            <Copy size={15} />
          </button>
          <span class="spacer" />
          <span class="history-path" title={props.tab.path}>
            {props.tab.path}, {lines()}
          </span>
        </div>
        <div class="history-columns">
          <div>Commit</div>
          <div>Author</div>
          <div>Date</div>
          <div>Hash</div>
        </div>
        <div
          class="log-list history-list"
          tabIndex={0}
          ref={list}
          onKeyDown={listKeys(() => (revs() ?? []).map((r) => r.sha), selectedSha, setSelectedSha, () => list)}
        >
          <For each={revs() ?? []}>
            {(r) => (
              <RevisionRow
                rev={r}
                selected={selectedSha() === r.sha}
                renamed={r.path !== props.tab.path ? r.path : undefined}
                onSelect={() => setSelectedSha(r.sha)}
                onMenu={menu}
                onOpen={showDiff}
              />
            )}
          </For>
          <Show when={revs.error}>
            <div class="commit-empty">{String(revs.error instanceof Error ? revs.error.message : revs.error)}</div>
          </Show>
          <Show when={revs() && !revs()!.length}>
            <div class="commit-empty">No history for the selected lines</div>
          </Show>
        </div>
        <div class="history-details">
          <CommitMessage sha={selectedSha()} />
        </div>
      </div>
      <Show
        when={current()}
        fallback={
          <div class="inline-diff">
            <div class="commit-empty">Select a revision to view how the lines changed</div>
          </div>
        }
      >
        {(r) => (
          <InlineDiff
            path={r().path}
            left={{
              title: r().before === null ? "Empty (lines added)" : r().parents[0] ? shortSha(r().parents[0]) : "Empty",
              content: r().before ?? "",
            }}
            right={{ title: `${shortSha(r().sha)} · ${r().path}, from line ${r().startLine}`, content: r().after }}
          />
        )}
      </Show>
    </div>
  );
}
