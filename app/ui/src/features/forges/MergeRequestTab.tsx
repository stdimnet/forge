/**
 * Merge/pull request details as an editor tab: header with actions (Approve, Merge…,
 * Close, Check out), Overview (description, timeline, comment box, reviewers, pipeline) and
 * Files (changed files tree with the diff and inline review threads).
 */
import { createEffect, createMemo, createResource, createSignal, For, Match, on, Show, Switch } from "solid-js";
import {
  Check,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  CircleCheck,
  Columns2,
  ExternalLink,
  Folder,
  GitBranch,
  GitMerge,
  MessageSquare,
  RefreshCw,
  ThumbsUp,
  X,
} from "lucide-solid";
import { forgesApi, type ChangedFile, type Discussion, type MergeRequestDetails, type Position, type ReviewState } from "@/ipc/forges";
import { FileIcon } from "@/shell/FileIcon";
import { approve, checkoutRequest, confirmClose, run, setDialog, showRequestDiff } from "./actions";
import { caps, errorText, me, ref, requestName, revision } from "./store";
import { CommentEditor, Markdown, NoteView, SystemNote, ThreadView } from "./Thread";
import { ReviewDiff } from "./ReviewDiff";
import { Avatar, BranchPill, CiIcon, ciLabels, LabelPill, MrStateIcon, StateBadge, ago, displayName } from "./ui";

type View = "overview" | "files";

const reviewText: Record<ReviewState, string> = {
  requested: "Review requested",
  approved: "Approved",
  changesRequested: "Changes requested",
  commented: "Commented",
};

export function MergeRequestTab(props: { number: number }) {
  const [view, setView] = createSignal<View>("overview");
  const [mr, { refetch: refetchMr }] = createResource(() => props.number, forgesApi.mergeRequest);
  const [discussions, { refetch: refetchDiscussions }] = createResource(() => props.number, forgesApi.discussions);
  const [files] = createResource(() => (view() === "files" || filesWanted() ? props.number : false), forgesApi.files);
  const [filesWanted, setFilesWanted] = createSignal(false);
  const [selectedFile, setSelectedFile] = createSignal<string | null>(null);

  const reload = () => {
    void refetchMr();
    void refetchDiscussions();
  };
  createEffect(on(revision, reload, { defer: true }));

  const comment = async (body: string, position?: Position) => {
    const ok = await run("Comment failed", () => forgesApi.comment(props.number, body, position));
    if (ok) void refetchDiscussions();
    return ok;
  };
  const reply = async (d: Discussion, body: string) => {
    const ok = await run("Reply failed", () => forgesApi.reply(props.number, d.id, body));
    if (ok) void refetchDiscussions();
    return ok;
  };
  const resolve = caps().resolveThreads
    ? async (d: Discussion, resolved: boolean) => {
        if (await run(resolved ? "Resolve failed" : "Unresolve failed", () => forgesApi.resolve(props.number, d.id, resolved))) void refetchDiscussions();
      }
    : undefined;

  const openFileThread = (path: string) => {
    setSelectedFile(path);
    setView("files");
  };

  const threadCount = (path: string) => (discussions() ?? []).filter((d) => d.position?.path === path && !d.resolved).length;

  return (
    <div class="fg-mr" data-mr-tab={props.number}>
      <Switch>
        <Match when={mr.error}>
          <div class="fg-empty">
            <div class="fg-error-text">{errorText(mr.error)}</div>
            <button class="btn-secondary" onClick={reload}>
              Retry
            </button>
          </div>
        </Match>
        <Match when={!mr()}>
          <div class="fg-empty muted">Loading {requestName().toLowerCase()} {ref(props.number)}…</div>
        </Match>
        <Match when={mr()}>
          {(d) => (
            <>
              <Header mr={d()} reload={reload} />
              <div class="git-tabs fg-mr-tabs">
                <button class="git-tab" classList={{ active: view() === "overview" }} onClick={() => setView("overview")}>
                  Overview
                </button>
                <button
                  class="git-tab"
                  classList={{ active: view() === "files" }}
                  onClick={() => setView("files")}
                  onMouseEnter={() => setFilesWanted(true)}
                  data-view="files"
                >
                  Files
                  <Show when={files()}>
                    <span class="fg-count">{files()!.length}</span>
                  </Show>
                </button>
              </div>
              <Show when={view() === "overview"}>
                <Overview mr={d()} discussions={discussions() ?? []} comment={comment} reply={reply} resolve={resolve} openFile={openFileThread} />
              </Show>
              <Show when={view() === "files"}>
                <FilesView
                  mr={d()}
                  files={files()}
                  loading={files.loading}
                  error={files.error ? errorText(files.error) : null}
                  discussions={discussions() ?? []}
                  selected={selectedFile()}
                  select={setSelectedFile}
                  threadCount={threadCount}
                  comment={comment}
                  reply={reply}
                  resolve={resolve}
                />
              </Show>
            </>
          )}
        </Match>
      </Switch>
    </div>
  );
}

function Header(props: { mr: MergeRequestDetails; reload: () => void }) {
  const mr = () => props.mr;
  const open = () => mr().state === "open";
  const approvedByMe = () => !!me() && mr().approvals.approvedBy.some((u) => u.username === me());
  const mine = () => mr().author.username === me();
  return (
    <header class="fg-mr-header">
      <div class="fg-mr-title-row">
        <MrStateIcon mr={mr()} size={18} />
        <h1 class="fg-mr-title">
          {mr().title} <span class="muted">{ref(mr().number)}</span>
        </h1>
        <StateBadge mr={mr()} />
      </div>
      <div class="fg-mr-sub">
        <Avatar user={mr().author} size={18} />
        <span class="fg-note-author">{displayName(mr().author)}</span>
        <span class="muted">{mr().state === "merged" ? "merged" : "wants to merge"}</span>
        <BranchPill name={mr().sourceBranch} />
        <span class="muted">into</span>
        <BranchPill name={mr().targetBranch} />
        <span class="muted">· opened {ago(mr().createdAt)}</span>
      </div>
      <div class="fg-mr-actions">
        <Show when={open()}>
          <Show when={caps().approvals && !mine()}>
            <button class="btn-secondary" disabled={approvedByMe()} onClick={() => void approve(mr(), props.reload)} data-action="approve">
              <Show when={approvedByMe()} fallback={<ThumbsUp size={14} />}>
                <Check size={14} />
              </Show>
              {approvedByMe() ? "Approved" : "Approve"}
            </button>
          </Show>
          <button class="btn-primary" disabled={mr().draft} title={mr().draft ? "Mark as ready before merging" : undefined} onClick={() => setDialog({ kind: "merge", mr: mr(), onDone: props.reload })} data-action="merge">
            <GitMerge size={14} />
            Merge…
          </button>
          <button class="btn-secondary" onClick={() => confirmClose(mr(), props.reload)} data-action="close">
            <X size={14} />
            Close
          </button>
        </Show>
        <button class="btn-secondary" onClick={() => void checkoutRequest(mr())} title={`Check out ${mr().sourceBranch}`} data-action="checkout">
          <GitBranch size={14} />
          Check Out
        </button>
        <span class="spacer" />
        <button class="icon-btn" title="Open in Browser" onClick={() => window.open(mr().webUrl, "_blank")}>
          <ExternalLink size={15} />
        </button>
        <button class="icon-btn" title="Refresh" onClick={props.reload}>
          <RefreshCw size={14} />
        </button>
      </div>
    </header>
  );
}

function Overview(props: {
  mr: MergeRequestDetails;
  discussions: Discussion[];
  comment: (body: string) => Promise<boolean>;
  reply: (d: Discussion, body: string) => Promise<boolean>;
  resolve?: (d: Discussion, resolved: boolean) => Promise<void>;
  openFile: (path: string) => void;
}) {
  const mr = () => props.mr;
  return (
    <div class="fg-overview">
      <div class="fg-overview-main">
        <div class="fg-card fg-description">
          <div class="fg-note-head">
            <Avatar user={mr().author} size={20} />
            <span class="fg-note-author">{displayName(mr().author)}</span>
            <span class="muted fg-small">opened {ago(mr().createdAt)}</span>
          </div>
          <Markdown text={mr().description} class="fg-description-body" />
        </div>
        <div class="fg-timeline">
          <For each={props.discussions}>
            {(d) => (
              <Switch>
                <Match when={d.notes.length === 1 && d.notes[0].system}>
                  <SystemNote note={d.notes[0]} />
                </Match>
                <Match when={d.position}>
                  <ThreadView
                    discussion={d}
                    showLocation
                    onOpenFile={() => props.openFile(d.position!.path)}
                    onReply={(b) => props.reply(d, b)}
                    onResolve={props.resolve ? (r) => props.resolve!(d, r) : undefined}
                  />
                </Match>
                <Match when={true}>
                  <div class="fg-card">
                    <For each={d.notes}>{(n) => (n.system ? <SystemNote note={n} /> : <NoteView note={n} />)}</For>
                  </div>
                </Match>
              </Switch>
            )}
          </For>
        </div>
        <div class="fg-card fg-new-comment">
          <CommentEditor placeholder="Leave a comment…" submitLabel="Comment" onSubmit={(b) => props.comment(b)} />
        </div>
      </div>
      <aside class="fg-sidebar">
        <Section title="Reviewers">
          <Show when={mr().reviewers.length} fallback={<div class="muted fg-small">No reviewers</div>}>
            <For each={mr().reviewers}>
              {(r) => (
                <div class="fg-side-row" title={reviewText[r.state]}>
                  <Avatar user={r.user} size={18} />
                  <span class="fg-side-name">{displayName(r.user)}</span>
                  <span class={`fg-review fg-review-${r.state}`}>
                    <Switch>
                      <Match when={r.state === "approved"}>
                        <CircleCheck size={14} />
                      </Match>
                      <Match when={r.state === "changesRequested"}>
                        <CircleAlert size={14} />
                      </Match>
                      <Match when={r.state === "commented"}>
                        <MessageSquare size={13} />
                      </Match>
                    </Switch>
                  </span>
                </div>
              )}
            </For>
          </Show>
          <Show when={caps().approvals && (mr().approvals.required || mr().approvals.approvedBy.length)}>
            <div class="muted fg-small fg-approvals">
              {mr().approvals.approvedBy.length}
              {mr().approvals.required ? ` of ${mr().approvals.required}` : ""} approval{(mr().approvals.required || mr().approvals.approvedBy.length) === 1 ? "" : "s"}
            </div>
          </Show>
        </Section>
        <Section title="Assignees">
          <Show when={mr().assignees.length} fallback={<div class="muted fg-small">None</div>}>
            <For each={mr().assignees}>
              {(u) => (
                <div class="fg-side-row">
                  <Avatar user={u} size={18} />
                  <span class="fg-side-name">{displayName(u)}</span>
                </div>
              )}
            </For>
          </Show>
        </Section>
        <Show when={mr().labels.length}>
          <Section title="Labels">
            <div class="fg-labels">
              <For each={mr().labels}>{(l) => <LabelPill label={l} />}</For>
            </div>
          </Section>
        </Show>
        <Section title="Pipeline">
          <Show when={mr().pipeline} fallback={<div class="muted fg-small">No pipeline for this commit</div>}>
            {(p) => (
              <>
                <div class="fg-side-row fg-pipeline-head">
                  <CiIcon status={p().status} size={15} />
                  <span class="fg-side-name">{ciLabels[p().status]}</span>
                  <Show when={p().webUrl}>
                    <button class="icon-btn fg-tiny-btn" title="Open in Browser" onClick={() => window.open(p().webUrl!, "_blank")}>
                      <ExternalLink size={13} />
                    </button>
                  </Show>
                </div>
                <For each={p().jobs}>
                  {(j) => (
                    <div class="fg-job" title={j.stage ?? undefined} onClick={() => j.webUrl && window.open(j.webUrl, "_blank")}>
                      <CiIcon status={j.status} size={13} />
                      <span class="fg-job-name">{j.name}</span>
                      <Show when={j.duration}>
                        <span class="muted fg-small">{formatDuration(j.duration!)}</span>
                      </Show>
                    </div>
                  )}
                </For>
              </>
            )}
          </Show>
        </Section>
        <Show when={mr().state === "open"}>
          <Section title="Merge">
            <div class="fg-side-row">
              <Switch fallback={<span class="muted fg-small">Checking mergeability…</span>}>
                <Match when={mr().conflicts}>
                  <span class="fg-review fg-review-changesRequested">
                    <CircleAlert size={14} />
                  </span>
                  <span class="fg-small">Has conflicts with {mr().targetBranch}</span>
                </Match>
                <Match when={mr().draft}>
                  <span class="muted fg-small">Draft: mark as ready to merge</span>
                </Match>
                <Match when={mr().mergeable === true}>
                  <span class="fg-review fg-review-approved">
                    <CircleCheck size={14} />
                  </span>
                  <span class="fg-small">No conflicts with {mr().targetBranch}</span>
                </Match>
              </Switch>
            </div>
          </Section>
        </Show>
      </aside>
    </div>
  );
}

const formatDuration = (s: number) => (s >= 60 ? `${Math.floor(s / 60)}m ${Math.round(s % 60)}s` : `${Math.round(s)}s`);

function Section(props: { title: string; children: unknown }) {
  return (
    <section class="fg-section">
      <div class="fg-section-title">{props.title}</div>
      {props.children as never}
    </section>
  );
}

interface TreeGroup {
  dir: string;
  files: ChangedFile[];
}

function FilesView(props: {
  mr: MergeRequestDetails;
  files: ChangedFile[] | undefined;
  loading: boolean;
  error: string | null;
  discussions: Discussion[];
  selected: string | null;
  select: (path: string) => void;
  threadCount: (path: string) => number;
  comment: (body: string, position?: Position) => Promise<boolean>;
  reply: (d: Discussion, body: string) => Promise<boolean>;
  resolve?: (d: Discussion, resolved: boolean) => Promise<void>;
}) {
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const groups = createMemo<TreeGroup[]>(() => {
    const map = new Map<string, ChangedFile[]>();
    for (const f of props.files ?? []) {
      const dir = f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : "";
      map.set(dir, [...(map.get(dir) ?? []), f]);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([dir, files]) => ({ dir, files }));
  });
  const current = () => props.files?.find((f) => f.path === props.selected) ?? props.files?.[0];
  createEffect(() => {
    const f = current();
    if (f && props.selected !== f.path) props.select(f.path);
  });
  const totals = () => (props.files ?? []).reduce((t, f) => [t[0] + f.additions, t[1] + f.deletions], [0, 0]);
  const toggle = (dir: string) =>
    setCollapsed((s) => {
      const n = new Set(s);
      n.has(dir) ? n.delete(dir) : n.add(dir);
      return n;
    });

  return (
    <Switch>
      <Match when={props.error}>
        <div class="fg-empty fg-error-text">{props.error}</div>
      </Match>
      <Match when={!props.files}>
        <div class="fg-empty muted">Loading changes…</div>
      </Match>
      <Match when={props.files}>
        <div class="fg-files">
          <div class="fg-files-tree">
            <div class="fg-files-summary muted fg-small">
              {props.files!.length} file{props.files!.length === 1 ? "" : "s"} changed
              <span class="fg-add">+{totals()[0]}</span>
              <span class="fg-del">−{totals()[1]}</span>
            </div>
            <For each={groups()}>
              {(g) => (
                <>
                  <Show when={g.dir}>
                    <div class="tree-row fg-tree-dir" onClick={() => toggle(g.dir)}>
                      <Show when={collapsed().has(g.dir)} fallback={<ChevronDown size={13} class="muted" />}>
                        <ChevronRight size={13} class="muted" />
                      </Show>
                      <Folder size={14} class="muted" />
                      <span class="tree-label">{g.dir}</span>
                    </div>
                  </Show>
                  <Show when={!collapsed().has(g.dir)}>
                    <For each={g.files}>
                      {(f) => {
                        const name = f.path.slice(f.path.lastIndexOf("/") + 1);
                        return (
                          <div
                            class="tree-row fg-tree-file"
                            classList={{ selected: current()?.path === f.path, nested: !!g.dir }}
                            data-file={f.path}
                            title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path}
                            onClick={() => props.select(f.path)}
                            onDblClick={() => void showRequestDiff(props.mr, f)}
                          >
                            <FileIcon name={name} size={14} />
                            <span class={`tree-label vcs-${f.status === "renamed" ? "modified" : f.status}`}>{name}</span>
                            <span class="spacer" />
                            <Show when={props.threadCount(f.path)}>
                              <span class="fg-file-threads" title="Unresolved threads">
                                <MessageSquare size={11} />
                                {props.threadCount(f.path)}
                              </span>
                            </Show>
                            <span class="fg-add fg-small">+{f.additions}</span>
                            <span class="fg-del fg-small">−{f.deletions}</span>
                          </div>
                        );
                      }}
                    </For>
                  </Show>
                </>
              )}
            </For>
          </div>
          <div class="fg-files-diff">
            <Show when={current()}>
              {(f) => (
                <>
                  <div class="fg-diff-header">
                    <FileIcon name={f().path.slice(f().path.lastIndexOf("/") + 1)} size={14} />
                    <span class="fg-diff-path">{f().oldPath ? `${f().oldPath} → ${f().path}` : f().path}</span>
                    <span class="fg-add fg-small">+{f().additions}</span>
                    <span class="fg-del fg-small">−{f().deletions}</span>
                    <span class="spacer" />
                    <button class="pill-btn" title="Show Diff (side by side)" onClick={() => void showRequestDiff(props.mr, f())}>
                      <Columns2 size={14} />
                      Show Diff
                    </button>
                  </div>
                  <div class="fg-diff-scroll">
                    <ReviewDiff
                      file={f()}
                      discussions={props.discussions}
                      canComment={props.mr.state === "open"}
                      onComment={(body, pos) => props.comment(body, pos)}
                      onReply={props.reply}
                      onResolve={props.resolve}
                    />
                  </div>
                </>
              )}
            </Show>
          </div>
        </div>
      </Match>
    </Switch>
  );
}
