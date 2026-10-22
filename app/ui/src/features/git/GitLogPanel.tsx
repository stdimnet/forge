/** Git tool window (Alt+9), Log tab: branches | commit graph | details. */
import { createEffect, createMemo, createResource, createSignal, For, on, Show } from "solid-js";
import {
  ChevronDown,
  ChevronRight,
  Cloud,
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
import { gitApi, type Branches, type Commit, type RefLabel } from "./api";
import { revision, status } from "./store";
import { layoutGraph, type GraphRow } from "./graph";
import { fullDate, logDate } from "./format";
import { checkout, fetchAll, showCommitDiff } from "./actions";
import { GitTabs, HistoryTabs } from "./FileHistoryPanel";
import { activeGitTab } from "./history";

const PAGE = 200;
const LANE = 14;
const ROW = 26;

const [filterRev, setFilterRev] = createSignal<string | null>(null);
const [filterText, setFilterText] = createSignal("");
/** Paths filter ("Show History" of a file or folder); `null` shows the whole repository. */
const [filterPath, setFilterPath] = createSignal<string | null>(null);
export { setFilterPath as setLogPathFilter };
const [selectedSha, setSelectedSha] = createSignal<string | null>(null);
export { setSelectedSha as selectLogCommit };

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
          return <path d={d} stroke={`var(--graph-${l.color % 7})`} stroke-width="1.6" fill="none" />;
        }}
      </For>
      <circle
        cx={x(props.row.lane)}
        cy={ROW / 2}
        r="4"
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

function BranchTree(props: { branches: Branches | undefined }) {
  const [closed, setClosed] = createSignal<Set<string>>(new Set(["Tags"]));
  const toggle = (k: string) =>
    setClosed((p) => {
      const n = new Set(p);
      n.has(k) ? n.delete(k) : n.add(k);
      return n;
    });
  const remotes = createMemo(() => {
    const map = new Map<string, string[]>();
    for (const b of props.branches?.remote ?? []) {
      const [remote, ...rest] = b.name.split("/");
      map.set(remote, [...(map.get(remote) ?? []), rest.join("/")]);
    }
    return [...map.entries()];
  });
  const Node = (p: { label: string; rev: string | null; icon: "branch" | "head" | "tag"; depth: number; current?: boolean; remote?: boolean }) => (
    <div
      class="tree-row"
      classList={{ selected: filterRev() === p.rev }}
      style={{ "padding-left": `${8 + p.depth * 16}px` }}
      onClick={() => setFilterRev(p.rev)}
      onDblClick={() => p.rev && p.icon === "branch" && !p.current && void checkout(p.rev, p.remote)}
      title={p.rev ?? "Show all branches"}
    >
      <span class="tree-chevron" />
      <Show when={p.icon === "head"} fallback={p.icon === "tag" ? <Tag size={14} class="muted" /> : <GitBranch size={14} class="muted" />}>
        <Monitor size={14} class="muted" />
      </Show>
      <span class="tree-label">{p.label}</span>
      <Show when={p.current}>
        <Star size={12} class="log-current-star" fill="currentColor" />
      </Show>
    </div>
  );
  const Section = (p: { label: string; depth: number; icon?: "cloud"; children: any }) => (
    <>
      <div class="tree-row" style={{ "padding-left": `${8 + p.depth * 16}px` }} onClick={() => toggle(p.label)}>
        <span class="tree-chevron">{closed().has(p.label) ? <ChevronRight size={14} /> : <ChevronDown size={14} />}</span>
        <Show when={p.icon === "cloud"}>
          <Cloud size={14} class="muted" />
        </Show>
        <span class="tree-label">{p.label}</span>
      </div>
      <Show when={!closed().has(p.label)}>{p.children}</Show>
    </>
  );
  return (
    <div class="log-branches">
      <Node label="HEAD (Current Branch)" rev="HEAD" icon="head" depth={0} />
      <Section label="Local" depth={0}>
        <For each={props.branches?.local ?? []}>
          {(b) => <Node label={b.name} rev={b.name} icon="branch" depth={1} current={b.current} />}
        </For>
      </Section>
      <Section label="Remote" depth={0}>
        <For each={remotes()}>
          {([remote, names]) => (
            <Section label={remote} depth={1} icon="cloud">
              <For each={names}>{(n) => <Node label={n} rev={`${remote}/${n}`} icon="branch" depth={2} remote />}</For>
            </Section>
          )}
        </For>
      </Section>
      <Section label="Tags" depth={0}>
        <For each={props.branches?.tags ?? []}>{(t) => <Node label={t.name} rev={t.name} icon="tag" depth={1} />}</For>
      </Section>
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
      <Show when={details()} keyed fallback={<div class="commit-empty">{props.sha ? "Loading…" : "Select a commit to view changes"}</div>}>
        {(detail) => {
          const d = () => detail;
          const [subject, ...body] = d().message.split("\n");
          return (
            <>
              <div class="log-files">
                <div class="log-files-title">
                  {d().files.length} file{d().files.length === 1 ? "" : "s"} changed
                </div>
                <For each={d().files}>
                  {(f) => (
                    <div
                      class="tree-row"
                      style={{ "padding-left": "10px" }}
                      title={`${f.path} — double-click to show diff`}
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
                  <span class="muted">&lt;{d().email}&gt;</span> on {fullDate(d().time)}
                </div>
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
      if (reset && !commits().some((c) => c.sha === selectedSha())) setSelectedSha(page[0]?.sha ?? null);
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

  const onKey = (e: KeyboardEvent) => {
    const list = commits();
    const i = list.findIndex((c) => c.sha === selectedSha());
    if (e.key === "ArrowDown" && i < list.length - 1) setSelectedSha(list[i + 1].sha);
    else if (e.key === "ArrowUp" && i > 0) setSelectedSha(list[i - 1].sha);
    else return;
    e.preventDefault();
  };

  return (
    <div class="git-panel">
      <GitTabs />
      <div class="git-log" classList={{ hidden: activeGitTab() !== "log" }}>
        <BranchTree branches={branches()} />
        <div class="log-main">
          <div class="panel-toolbar log-filters">
            <label class="log-search">
              <Search size={13} class="muted" />
              <input
                placeholder="Text or hash"
                value={filterText()}
                onInput={(e) => {
                  const v = e.currentTarget.value;
                  clearTimeout(filterTimer);
                  filterTimer = window.setTimeout(() => setFilterText(v), 250);
                }}
              />
            </label>
            <button class="pill-btn log-filter" onClick={() => setFilterRev(null)} title="Branch filter">
              Branch: <b>{filterRev() ?? "All"}</b>
              <ChevronDown size={12} class="muted" />
            </button>
            <button class="pill-btn log-filter" title="User filter">
              User <ChevronDown size={12} class="muted" />
            </button>
            <button class="pill-btn log-filter" title="Date filter">
              Date <ChevronDown size={12} class="muted" />
            </button>
            <Show
              when={filterPath() !== null}
              fallback={
                <button class="pill-btn log-filter" title="Paths filter">
                  Paths <ChevronDown size={12} class="muted" />
                </button>
              }
            >
              <button class="pill-btn log-filter" title={`Paths: ${filterPath() || "/"} (click to clear)`} onClick={() => setFilterPath(null)}>
                Paths: <b>{filterPath()!.split("/").pop() || "/"}</b>
                <X size={12} class="muted" />
              </button>
            </Show>
            <span class="spacer" />
            <button class="icon-btn" title="Fetch All Remotes" onClick={() => void fetchAll()}>
              <Download size={15} />
            </button>
            <button class="icon-btn" title="Refresh (Ctrl+F5)" onClick={() => void load(true)}>
              <RefreshCw size={15} />
            </button>
          </div>
          <div
            class="log-list"
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
                    selected: selectedSha() === c.sha,
                    head: c.sha === status()?.head,
                    merge: c.parents.length > 1,
                  }}
                  data-sha={c.sha}
                  onClick={() => setSelectedSha(c.sha)}
                >
                  <div class="log-subject-cell">
                    <Graph row={graph()[i()]} width={graphWidth()} />
                    <span class="log-subject-text">{c.subject}</span>
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
              <div class="commit-empty">No commits matching filters</div>
            </Show>
          </div>
        </div>
        <Details sha={selectedSha()} commit={selected()} />
      </div>
      <HistoryTabs />
    </div>
  );
}
