/**
 * Git tool window (Alt+9), Log tab: branches | commit graph | details. The tabs (Log,
 * Worktrees, history tabs) are in the tool window header (`GitTabs`), so the window has a
 * single compact toolbar row. Commits are selected
 * with click, Ctrl+click and Shift+click; the context menu holds the Log actions
 * (`logActions.ts`).
 */
import { createEffect, createMemo, createResource, createSignal, For, Match, on, onCleanup, Show, Switch } from "solid-js";
import {
  ChevronDown,
  Cloud,
  Folder,
  GitBranch,
  Monitor,
  RefreshCw,
  Search,
  Star,
  Tag,
  Download,
  X,
} from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { TreeArrow } from "@/shell/TreeArrow";
import { navigate } from "@/features/project/treeNavigation";
import { gitApi, type BranchNode, type Branches, type Commit, type RefLabel } from "./api";
import { flattenBranchTree, groupByDirectory, isFolder, toggled, ungrouped, type Expansion } from "./branchTree";
import { revision, status } from "./store";
import { layoutGraph, type GraphRow } from "./graph";
import { fullDate, logDate } from "./format";
import { checkout, fetchAll, showCommitDiff } from "./actions";
import { HistoryTabs } from "./FileHistoryPanel";
import { activeGitTab, setActiveGitTab } from "./history";
import { goToRelative, openLogMenu, setLogView } from "./logActions";
import { showToolWindow } from "@/state/layout";
import { WorktreesPanel } from "./ExtrasDialogs";
import { SignatureIcon, signatureLabel } from "./signature";
import { t as tr } from "@/i18n";

const PAGE = 200;
// Compact rows like PHPStorm's Log; keep in sync with `.log-row` in git.css.
const LANE = 12;
const ROW = 22;

const [filterRev, setFilterRev] = createSignal<string | null>(null);
const [filterText, setFilterText] = createSignal("");
/** Paths filter ("Show History" of a file or folder); `null` shows the whole repository. */
const [filterPath, setFilterPath] = createSignal<string | null>(null);
export { setFilterPath as setLogPathFilter };
/** The lead selection (details pane) and every selected commit. */
const [selectedSha, setSelectedSha] = createSignal<string | null>(null);
const [selectedSet, setSelectedSet] = createSignal<Set<string>>(new Set());

/** Select one commit (and only that one). */
export function selectLogCommit(sha: string | null): void {
  setSelectedSha(sha);
  setSelectedSet(new Set(sha ? [sha] : []));
}

/** Show the log of a branch or range (`main..feature`) in the Git tool window. */
export function showLogFor(rev: string | null): void {
  setFilterRev(rev);
  setActiveGitTab("log");
  showToolWindow("git");
}

function Graph(props: { row: GraphRow; width: number }) {
  const x = (lane: number) => lane * LANE + LANE / 2 + 2;
  const y = (v: number) => v * ROW;
  return (
    <svg class="log-graph" width={props.width * LANE + 6} height={ROW} aria-hidden="true">
      <For each={props.row.lines}>
        {(l) => {
          const x1 = x(l.x1);
          const x2 = x(l.x2);
          const y1 = y(l.y1);
          const y2 = y(l.y2);
          const d =
            x1 === x2
              ? `M${x1} ${y1}L${x2} ${y2}`
              : `M${x1} ${y1}C${x1} ${(y1 + y2) / 2} ${x2} ${(y1 + y2) / 2} ${x2} ${y2}`;
          return <path d={d} stroke={`var(--graph-${l.color % 7})`} stroke-width="1.5" fill="none" />;
        }}
      </For>
      <circle
        cx={x(props.row.lane)}
        cy={ROW / 2}
        r="3.5"
        fill={`var(--graph-${props.row.color % 7})`}
        stroke="var(--bg-panel)"
        stroke-width="1.5"
      />
    </svg>
  );
}

function RefPill(props: { r: RefLabel }) {
  return (
    <span class={`ref-pill ref-${props.r.kind}`} classList={{ current: props.r.current }} title={props.r.name}>
      <Show when={props.r.kind === "tag"} fallback={<GitBranch size={11} />}>
        <Tag size={11} />
      </Show>
      {props.r.name}
    </span>
  );
}

/** A row of the branches tree: HEAD, a section, a remote or folder, a branch or a tag. */
interface BranchTreeItem {
  key: string;
  kind: "head" | "section" | "remote" | "folder" | "branch" | "tag";
  label: string;
  depth: number;
  /** What the row filters the log by; folders and sections filter nothing. */
  rev?: string;
  expanded?: boolean;
  current?: boolean;
  remote?: boolean;
}

/** Element id of a row (`aria-activedescendant`). */
const branchRowIds = new Map<string, string>();
const branchRowId = (key: string): string => {
  let id = branchRowIds.get(key);
  if (!id) branchRowIds.set(key, (id = `log-branch-row-${branchRowIds.size + 1}`));
  return id;
};

function BranchTree(props: { branches: Branches | undefined }) {
  /** Opened and closed rows; sections start open except Tags, folders as in the Branches popup. */
  const [expansion, setExpansion] = createSignal<Expansion>(new Map([["section:tags", false]]));
  /** The row the arrow keys are on. */
  const [cursor, setCursor] = createSignal<string | null>(null);

  const rows = createMemo(() => {
    const out: BranchTreeItem[] = [{ key: "head", kind: "head", label: tr("git.gitLogPanel.headCurrentBranch"), depth: 0, rev: "HEAD" }];
    const section = (id: string, label: string, children: () => BranchTreeItem[]) => {
      const key = `section:${id}`;
      const expanded = expansion().get(key) ?? true;
      out.push({ key, kind: "section", label, depth: 0, expanded });
      if (expanded) out.push(...children());
    };
    const tree = (name: string, nodes: BranchNode[]) =>
      flattenBranchTree(nodes, { section: name, expansion: expansion(), depth: 1 }).map(
        ({ key, node, depth, expanded }): BranchTreeItem =>
          isFolder(node)
            ? { key, kind: node.kind === "remote" ? "remote" : "folder", label: node.label, depth, expanded }
            : { key, kind: "branch", label: node.label, depth, rev: node.path, current: node.current, remote: name === "remote" },
      );
    const b = props.branches;
    const grouped = groupByDirectory();
    section("local", tr("git.gitLogPanel.local"), () =>
      grouped
        ? tree("local", b?.localTree ?? [])
        : (b?.local ?? []).map((x): BranchTreeItem => ({ key: `local:branch:${x.name}`, kind: "branch", label: x.name, depth: 1, rev: x.name, current: x.current })),
    );
    section("remote", tr("git.gitLogPanel.remote"), () => tree("remote", grouped ? (b?.remoteTree ?? []) : ungrouped(b?.remoteTree ?? [], (b?.remote ?? []).map((x) => x.name))));
    section("tags", tr("git.gitLogPanel.tags"), () => (b?.tags ?? []).map((x): BranchTreeItem => ({ key: `tag:${x.name}`, kind: "tag", label: x.name, depth: 1, rev: x.name })));
    return out;
  });

  const isDir = (row: BranchTreeItem) => row.expanded !== undefined;
  const toggle = (row: BranchTreeItem, open = !row.expanded) => setExpansion((e) => toggled(e, { key: row.key, expanded: !!row.expanded }, open));
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const action = navigate(
      rows().map((r) => ({ path: r.key, isDir: isDir(r), expanded: !!r.expanded, depth: r.depth })),
      cursor(),
      e.key,
    );
    if (action.type === "none") return;
    e.preventDefault();
    const row = rows().find((r) => r.key === action.path);
    if (!row) return;
    if (action.type === "select") {
      setCursor(row.key);
      queueMicrotask(() => document.getElementById(branchRowId(row.key))?.scrollIntoView?.({ block: "nearest" }));
    } else if (action.type === "open") setFilterRev(row.rev ?? null);
    else toggle(row, action.type === "expand");
  };

  const Icon = (p: { row: BranchTreeItem }) => (
    <Switch>
      <Match when={p.row.kind === "head"}>
        <Monitor size={14} class="muted" aria-hidden="true" />
      </Match>
      <Match when={p.row.kind === "remote"}>
        <Cloud size={14} class="muted" aria-hidden="true" />
      </Match>
      <Match when={p.row.kind === "folder"}>
        <Folder size={14} class="muted" aria-hidden="true" />
      </Match>
      <Match when={p.row.kind === "branch"}>
        <GitBranch size={14} class="muted" aria-hidden="true" />
      </Match>
      <Match when={p.row.kind === "tag"}>
        <Tag size={14} class="muted" aria-hidden="true" />
      </Match>
    </Switch>
  );

  return (
    <div
      class="log-branches"
      tabIndex={0}
      role="tree"
      aria-label={tr("git.branchesPopup.branches")}
      aria-activedescendant={cursor() ? branchRowId(cursor()!) : undefined}
      onKeyDown={onKeyDown}
    >
      <For each={rows()}>
        {(row) => (
          <div
            class="tree-row"
            classList={{ selected: row.rev !== undefined && filterRev() === row.rev }}
            style={{ "padding-left": `${8 + row.depth * 16}px` }}
            id={branchRowId(row.key)}
            role="treeitem"
            aria-level={row.depth + 1}
            aria-expanded={isDir(row) ? row.expanded : undefined}
            aria-selected={cursor() === row.key}
            aria-current={row.current ? "true" : undefined}
            data-branch={row.kind === "branch" ? row.rev : undefined}
            data-folder={row.kind === "folder" || row.kind === "remote" ? row.key : undefined}
            title={row.rev ?? undefined}
            onClick={() => {
              setCursor(row.key);
              if (isDir(row)) toggle(row);
              else setFilterRev(row.rev ?? null);
            }}
            onDblClick={() => row.kind === "branch" && !row.current && void checkout(row.rev!, row.remote)}
          >
            <span class="tree-chevron" aria-hidden="true">
              <Show when={isDir(row)}>
                <TreeArrow open={!!row.expanded} />
              </Show>
            </span>
            <Icon row={row} />
            <span class="tree-label">{row.label}</span>
            <Show when={row.current}>
              <Star size={12} class="log-current-star" fill="currentColor" aria-hidden="true" />
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}

function Details(props: { sha: string | null; commit: Commit | undefined }) {
  const [details] = createResource(
    () => props.sha,
    (sha) => gitApi.commitDetails(sha),
  );
  const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
  const statusClass = (s: string) => (s === "A" ? "vcs-added" : s === "D" ? "vcs-deleted" : "vcs-modified");
  return (
    <div class="log-details">
      <Show when={details()} keyed fallback={<div class="commit-empty">{props.sha ? tr("git.gitLogPanel.loading") : tr("git.gitLogPanel.selectACommitToView")}</div>}>
        {(detail) => {
          const d = () => detail;
          const [subject, ...body] = d().message.split("\n");
          return (
            <>
              <div class="log-files">
                <div class="log-files-title">
                  {tr("common.filesChanged", { count: d().files.length })}
                </div>
                <For each={d().files}>
                  {(f) => (
                    <div
                      class="tree-row"
                      style={{ "padding-left": "10px" }}
                      title={tr("tools.git.doubleClickToShowDiff", { path: f.path })}
                      onDblClick={() => void showCommitDiff(d().sha, d().parents[0], f.path)}
                    >
                      <FileIcon name={f.path.split("/").pop()!} />
                      <span class={`tree-label ${statusClass(f.status)}`}>{f.path.split("/").pop()}</span>
                      <span class="tree-hint">{dirOf(f.path)}</span>
                    </div>
                  )}
                </For>
              </div>
              <div class="log-message">
                <div class="log-subject">{subject}</div>
                <Show when={body.join("\n").trim()}>
                  <p class="log-body">{body.join("\n").trim().replace(/([^\n])\n(?!\n)/g, "$1 ")}</p>
                </Show>
                <div class="log-meta">
                  <span class="log-sha">{d().sha.slice(0, 8)}</span> {d().author}{" "}
                  <span class="muted">{tr("tools.git.ltGt", { email: d().email })}</span> on {fullDate(d().time)}
                </div>
                <Show when={d().signature && d().signature !== "none"}>
                  <div class="log-meta log-signed" data-testid="log-signer">
                    <SignatureIcon status={d().signature} />
                    {signatureLabel(d().signature!)}
                    <Show when={d().signer}> by {d().signer}</Show>
                    <Show when={d().signingKey}>
                      {" "}
                      <span class="muted">({d().signingKey})</span>
                    </Show>
                  </div>
                </Show>
                <Show when={props.commit?.refs.length}>
                  <div class="log-refs">
                    <For each={props.commit!.refs}>{(r) => <RefPill r={r} />}</For>
                  </div>
                </Show>
              </div>
            </>
          );
        }}
      </Show>
    </div>
  );
}

export function GitLogPanel() {
  const [commits, setCommits] = createSignal<Commit[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [exhausted, setExhausted] = createSignal(false);
  const [branches, { refetch: refetchBranches }] = createResource(() => gitApi.branches().catch(() => undefined));

  const load = async (reset: boolean) => {
    if (loading() || (!reset && exhausted())) return;
    setLoading(true);
    try {
      const rev = filterRev();
      const page = await gitApi.log({
        skip: reset ? 0 : commits().length,
        limit: PAGE,
        all: rev === null,
        rev: rev,
        text: filterText() || null,
        path: filterPath(),
      });
      setExhausted(page.length < PAGE);
      setCommits(reset ? page : [...commits(), ...page]);
      if (reset && !commits().some((c) => c.sha === selectedSha())) selectLogCommit(page[0]?.sha ?? null);
      if (reset) setSelectedSet((set) => new Set([...set].filter((sha) => commits().some((c) => c.sha === sha))));
    } catch {
      setCommits([]);
    } finally {
      setLoading(false);
    }
  };

  createEffect(
    on([filterRev, filterText, filterPath, revision], () => {
      void load(true);
      void refetchBranches();
    }),
  );

  const graph = createMemo(() => layoutGraph(commits()));
  const graphWidth = createMemo(() => Math.max(1, ...graph().map((r) => r.width)));
  const selected = () => commits().find((c) => c.sha === selectedSha());
  let filterTimer: number | undefined;

  let listEl!: HTMLDivElement;
  const reveal = (sha: string) => queueMicrotask(() => listEl?.querySelector(`[data-sha="${sha}"]`)?.scrollIntoView({ block: "nearest" }));
  const selectedInOrder = () => commits().filter((c) => selectedSet().has(c.sha)).map((c) => c.sha);
  setLogView({
    commits,
    selected: selectedInOrder,
    select: (sha) => {
      selectLogCommit(sha);
      reveal(sha);
    },
  });
  onCleanup(() => setLogView(null));

  const click = (e: MouseEvent, c: Commit) => {
    const list = commits();
    const lead = selectedSha();
    if (e.shiftKey && lead) {
      const a = list.findIndex((x) => x.sha === lead);
      const b = list.indexOf(c);
      if (a >= 0) {
        setSelectedSet(new Set(list.slice(Math.min(a, b), Math.max(a, b) + 1).map((x) => x.sha)));
        return;
      }
    }
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(selectedSet());
      next.has(c.sha) ? next.delete(c.sha) : next.add(c.sha);
      setSelectedSet(next);
      setSelectedSha(c.sha);
      return;
    }
    selectLogCommit(c.sha);
  };

  /** Arrow keys move the selection; Left / Right go to the child / parent commit. */
  const onKey = (e: KeyboardEvent) => {
    const list = commits();
    const i = list.findIndex((c) => c.sha === selectedSha());
    const sha = selectedSha();
    if (e.key === "ArrowDown" && i < list.length - 1) selectLogCommit(list[i + 1].sha);
    else if (e.key === "ArrowUp" && i > 0) selectLogCommit(list[i - 1].sha);
    else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && sha && !e.altKey && !e.ctrlKey && !e.metaKey) goToRelative(sha, e.key === "ArrowRight");
    else return;
    e.preventDefault();
    if (selectedSha()) reveal(selectedSha()!);
  };

  return (
    <div class="git-panel">
      <div class="git-log" classList={{ hidden: activeGitTab() !== "log" }}>
        <BranchTree branches={branches()} />
        <div class="log-main">
          <div class="panel-toolbar log-filters">
            <label class="log-search">
              <Search size={13} class="muted" />
              <input
                placeholder={tr("git.gitLogPanel.textOrHash")}
                value={filterText()}
                onInput={(e) => {
                  const v = e.currentTarget.value;
                  clearTimeout(filterTimer);
                  filterTimer = window.setTimeout(() => setFilterText(v), 250);
                }}
              />
            </label>
            <button class="pill-btn log-filter" onClick={() => setFilterRev(null)} title={tr("git.gitLogPanel.branchFilter")}>
              {tr("git.gitLogPanel.branchLabel")} <b>{filterRev() ?? tr("git.gitLogPanel.all")}</b>
              <ChevronDown size={12} class="muted" />
            </button>
            <button class="pill-btn log-filter" title={tr("git.gitLogPanel.userFilter")}>
              {tr("git.gitLogPanel.user")} <ChevronDown size={12} class="muted" />
            </button>
            <button class="pill-btn log-filter" title={tr("git.gitLogPanel.dateFilter")}>
              {tr("git.gitLogPanel.date")} <ChevronDown size={12} class="muted" />
            </button>
            <Show
              when={filterPath() !== null}
              fallback={
                <button class="pill-btn log-filter" title={tr("git.gitLogPanel.pathsFilter")}>
                  {tr("git.gitLogPanel.paths")} <ChevronDown size={12} class="muted" />
                </button>
              }
            >
              <button class="pill-btn log-filter" title={tr("tools.git.pathsClickToClear", { value: filterPath() || "/" })} onClick={() => setFilterPath(null)}>
                {tr("tools.git.pathsLabel")} <b>{filterPath()!.split("/").pop() || "/"}</b>
                <X size={12} class="muted" />
              </button>
            </Show>
            <span class="spacer" />
            <button class="icon-btn" title={tr("git.gitLogPanel.fetchAllRemotes")} onClick={() => void fetchAll()}>
              <Download size={15} />
            </button>
            <button class="icon-btn" title={tr("git.gitLogPanel.refresh", { shortcut: "Ctrl+F5" })} onClick={() => void load(true)}>
              <RefreshCw size={15} />
            </button>
          </div>
          <div
            class="log-list"
            ref={listEl}
            tabIndex={0}
            onKeyDown={onKey}
            onScroll={(e) => {
              const el = e.currentTarget;
              if (el.scrollTop + el.clientHeight > el.scrollHeight - 200) void load(false);
            }}
          >
            <For each={commits()}>
              {(c, i) => (
                <div
                  class="log-row"
                  classList={{
                    selected: selectedSet().has(c.sha),
                    lead: selectedSha() === c.sha,
                    head: c.sha === status()?.head,
                    merge: c.parents.length > 1,
                  }}
                  data-sha={c.sha}
                  onClick={(e) => click(e, c)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    if (!selectedSet().has(c.sha)) selectLogCommit(c.sha);
                    void openLogMenu(e.clientX, e.clientY);
                  }}
                >
                  <div class="log-subject-cell">
                    <Graph row={graph()[i()]} width={graphWidth()} />
                    <span class="log-subject-text">{c.subject}</span>
                    <SignatureIcon status={c.signature} />
                    <span class="log-row-refs">
                      <For each={c.refs}>{(r) => <RefPill r={r} />}</For>
                    </span>
                  </div>
                  <div class="log-author">{c.author}</div>
                  <div class="log-date">{logDate(c.time)}</div>
                </div>
              )}
            </For>
            <Show when={!commits().length && !loading()}>
              <div class="commit-empty">{tr("git.gitLogPanel.noCommitsMatchingFilters")}</div>
            </Show>
          </div>
        </div>
        <Details sha={selectedSha()} commit={selected()} />
      </div>
      <Show when={activeGitTab() === "worktrees"}>
        <WorktreesPanel />
      </Show>
      <HistoryTabs />
    </div>
  );
}
