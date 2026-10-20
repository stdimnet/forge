/**
 * Pull Requests / Merge Requests tool window: search with filter chips, the request list
 * with CI status, comments and reviewers, plus Issues and Pipelines tabs.
 */
import { createEffect, createResource, createSignal, For, Match, on, Show, Switch } from "solid-js";
import { createStore } from "solid-js/store";
import { ChevronDown, CloudOff, ExternalLink, Filter, KeyRound, MessageSquare, Plus, RefreshCw, Search, X } from "lucide-solid";
import {
  forgesApi,
  type Issue,
  type IssueFilter,
  type MergeRequest,
  type MrFilter,
  type Pipeline,
  type StateFilter,
  type User,
} from "@/ipc/forges";
import { openSettings } from "@/features/settings/dialogState";
import { context, contextError, contextLoaded, me, ref, refreshAll, requestName, revision, errorText } from "./store";
import { Avatar, BranchPill, CiIcon, ciLabels, IssueStateIcon, MrStateIcon, ago, displayName } from "./ui";
import { Menu, type MenuItem } from "./Menu";
import { openCreateDialog, openIssue, openMergeRequest, settingsPageFor } from "./actions";

type Tab = "requests" | "issues" | "pipelines";

const [tab, setTab] = createSignal<Tab>("requests");
const [filter, setFilter] = createStore<MrFilter>({ state: "open" });
const [issueFilter, setIssueFilter] = createStore<IssueFilter>({ state: "open" });
const [selected, setSelected] = createSignal<number | null>(null);

export { setTab as setRequestsTab };

const stateNames: Record<StateFilter, string> = { open: "Open", merged: "Merged", closed: "Closed", all: "All" };

function useMenu() {
  const [menu, setMenu] = createSignal<{ anchor: HTMLElement; items: MenuItem[] } | null>(null);
  const open = (e: MouseEvent, items: MenuItem[]) => {
    const anchor = e.currentTarget as HTMLElement;
    setMenu((m) => (m?.anchor === anchor ? null : { anchor, items }));
  };
  const view = () => (
    <Show when={menu()}>{(m) => <Menu anchor={m().anchor} items={m().items} onClose={() => setMenu(null)} />}</Show>
  );
  return { open, view };
}

function Chip(props: { label: string; value?: string | null; onClick: (e: MouseEvent) => void; onClear?: () => void }) {
  return (
    <button class="fg-chip" classList={{ set: !!props.value }} onClick={(e) => props.onClick(e)} data-chip={props.label}>
      <span>{props.label}</span>
      <Show when={props.value}>
        <span class="fg-chip-value">{props.value}</span>
      </Show>
      <Show when={props.value && props.onClear} fallback={<ChevronDown size={12} />}>
        <span
          class="fg-chip-clear"
          title="Clear"
          onClick={(e) => {
            e.stopPropagation();
            props.onClear?.();
          }}
        >
          <X size={11} />
        </span>
      </Show>
    </button>
  );
}

function SearchBox(props: { value: string; placeholder: string; onChange: (v: string) => void }) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return (
    <label class="fg-search">
      <Search size={13} class="muted" />
      <input
        placeholder={props.placeholder}
        value={props.value}
        spellcheck={false}
        onInput={(e) => {
          const v = e.currentTarget.value;
          clearTimeout(timer);
          timer = setTimeout(() => props.onChange(v), 250);
        }}
      />
    </label>
  );
}

function NotConnected() {
  const account = () => context()?.account;
  return (
    <div class="fg-empty">
      <Switch>
        <Match when={!contextLoaded()}>
          <span class="muted">Loading…</span>
        </Match>
        <Match when={!context()}>
          <CloudOff size={28} class="muted" />
          <div>No GitLab, Forgejo or GitHub remote found in this project.</div>
          <Show when={contextError()}>
            <div class="muted fg-small">{contextError()}</div>
          </Show>
          <button class="btn-secondary" onClick={() => openSettings("vcs.gitlab")}>
            Configure Accounts…
          </button>
        </Match>
        <Match when={!account()}>
          <KeyRound size={28} class="muted" />
          <div>
            Log in to {context()!.repo.server.replace(/^https?:\/\//, "")} to review {requestName().toLowerCase()}s.
          </div>
          <button class="btn-primary" onClick={() => openSettings(settingsPageFor(context()!.repo.kind))}>
            Log In with Token…
          </button>
        </Match>
      </Switch>
    </div>
  );
}

function RequestRow(props: { mr: MergeRequest }) {
  const mr = () => props.mr;
  return (
    <div
      class="fg-row"
      classList={{ selected: selected() === mr().number }}
      data-mr={mr().number}
      tabindex="-1"
      title={`${mr().title}\n${mr().sourceBranch} → ${mr().targetBranch}${mr().labels.length ? `\nLabels: ${mr().labels.map((l) => l.name).join(", ")}` : ""}`}
      onClick={() => {
        setSelected(mr().number);
        openMergeRequest(mr().number, mr().title);
      }}
    >
      <MrStateIcon mr={mr()} />
      <div class="fg-row-main">
        <div class="fg-row-title">
          <span class="fg-row-title-text" classList={{ draft: mr().draft }}>
            {mr().title}
          </span>
        </div>
        <div class="fg-row-sub muted">
          <span class="fg-row-sub-text">
            {ref(mr().number)} {displayName(mr().author)} · {ago(mr().updatedAt)}
          </span>
        </div>
      </div>
      <div class="fg-row-meta">
        <Show when={mr().ci}>
          <CiIcon status={mr().ci} />
        </Show>
        <Show when={mr().comments}>
          <span class="fg-comments" title={`${mr().comments} comments`}>
            <MessageSquare size={13} />
            {mr().comments}
          </span>
        </Show>
        <Avatar user={mr().author} size={18} />
      </div>
    </div>
  );
}

function RequestsTab() {
  const menus = useMenu();
  const [members] = createResource(() => (context()?.account ? revision() + 1 : false), () => forgesApi.members().catch(() => [] as User[]));
  const [page, { refetch }] = createResource(
    () => (context()?.account ? { ...filter, rev: revision() } : false),
    ({ rev: _r, ...f }) => forgesApi.mergeRequests(f),
  );
  const [more, setMore] = createSignal<MergeRequest[]>([]);
  const [nextPage, setNextPage] = createSignal<number | null>(null);
  createEffect(
    on(page, (p) => {
      setMore([]);
      setNextPage(p?.nextPage ?? null);
    }),
  );
  const loadMore = async () => {
    const n = nextPage();
    if (!n) return;
    const p = await forgesApi.mergeRequests({ ...filter, page: n });
    setMore((m) => [...m, ...p.items]);
    setNextPage(p.nextPage);
  };
  const items = () => [...(page()?.items ?? []), ...more()];

  const userItems = (key: "author" | "reviewer" | "assignee"): MenuItem[] => {
    const current = filter[key];
    const list: MenuItem[] = [];
    if (me()) list.push({ label: "Me", hint: me()!, checked: current === me(), run: () => setFilter(key, me()) });
    for (const u of members() ?? [])
      if (u.username !== me()) list.push({ label: displayName(u), hint: u.username, checked: current === u.username, run: () => setFilter(key, u.username) });
    list.push({ separator: true, label: "" }, { label: "Any", checked: !current, run: () => setFilter(key, null) });
    return list;
  };
  const labelItems = (): MenuItem[] => {
    const names = [...new Set(items().flatMap((m) => m.labels.map((l) => l.name)))].sort();
    return [
      ...names.map((n) => ({ label: n, checked: filter.label === n, run: () => setFilter("label", n) })),
      { separator: true, label: "" },
      { label: "Any", checked: !filter.label, run: () => setFilter("label", null) },
    ];
  };
  const quick = (): MenuItem[] => [
    { label: `Open`, checked: filter.state === "open" && !filter.author && !filter.reviewer && !filter.assignee, run: () => applyQuick({ state: "open" }) },
    { label: `Your ${requestName().toLowerCase()}s`, run: () => applyQuick({ state: "open", author: me() }) },
    { label: "Assigned to you", run: () => applyQuick({ state: "open", assignee: me() }) },
    { label: "Review requests", run: () => applyQuick({ state: "open", reviewer: me() }) },
    { separator: true, label: "" },
    { label: "Merged", checked: filter.state === "merged", run: () => applyQuick({ state: "merged" }) },
    { label: "Closed", checked: filter.state === "closed", run: () => applyQuick({ state: "closed" }) },
    { label: "All", checked: filter.state === "all", run: () => applyQuick({ state: "all" }) },
  ];
  const applyQuick = (f: MrFilter) => setFilter({ state: "open", author: null, reviewer: null, assignee: null, label: null, ...f });
  const userName = (u: string | null | undefined) => (u && u === me() ? "me" : u);

  return (
    <>
      <div class="fg-filters">
        <div class="fg-search-row">
          <SearchBox value={filter.search ?? ""} placeholder="Search" onChange={(v) => setFilter("search", v || null)} />
          <button class="icon-btn" title="Quick Filters" onClick={(e) => menus.open(e, quick())}>
            <Filter size={14} />
          </button>
        </div>
        <div class="fg-chips">
          <Chip
            label="State"
            value={stateNames[filter.state ?? "open"]}
            onClick={(e) =>
              menus.open(
                e,
                (["open", "merged", "closed", "all"] as StateFilter[]).map((s) => ({
                  label: stateNames[s],
                  checked: (filter.state ?? "open") === s,
                  run: () => setFilter("state", s),
                })),
              )
            }
          />
          <Chip label="Author" value={userName(filter.author)} onClick={(e) => menus.open(e, userItems("author"))} onClear={() => setFilter("author", null)} />
          <Chip label="Label" value={filter.label} onClick={(e) => menus.open(e, labelItems())} onClear={() => setFilter("label", null)} />
          <Chip label="Assignee" value={userName(filter.assignee)} onClick={(e) => menus.open(e, userItems("assignee"))} onClear={() => setFilter("assignee", null)} />
          <Chip label="Reviewer" value={userName(filter.reviewer)} onClick={(e) => menus.open(e, userItems("reviewer"))} onClear={() => setFilter("reviewer", null)} />
        </div>
      </div>
      <div class="fg-list" role="list">
        <Switch>
          <Match when={page.error}>
            <div class="fg-empty">
              <div class="fg-error-text">{errorText(page.error)}</div>
              <button class="btn-secondary" onClick={() => void refetch()}>
                Retry
              </button>
            </div>
          </Match>
          <Match when={page.loading && !page()}>
            <div class="fg-empty muted">Loading {requestName().toLowerCase()}s…</div>
          </Match>
          <Match when={!items().length}>
            <div class="fg-empty muted">
              No {requestName().toLowerCase()}s
              <button class="fg-link" onClick={openCreateDialog}>
                Create {requestName()}…
              </button>
            </div>
          </Match>
          <Match when={true}>
            <For each={items()}>{(mr) => <RequestRow mr={mr} />}</For>
            <Show when={nextPage()}>
              <button class="fg-load-more" onClick={() => void loadMore()}>
                Load more…
              </button>
            </Show>
          </Match>
        </Switch>
      </div>
      {menus.view()}
    </>
  );
}

function IssuesTab() {
  const menus = useMenu();
  const [page, { refetch }] = createResource(
    () => (context()?.account ? { ...issueFilter, rev: revision() } : false),
    ({ rev: _r, ...f }) => forgesApi.issues(f),
  );
  const stateItems = (): MenuItem[] =>
    (["open", "closed", "all"] as const).map((s) => ({
      label: s === "all" ? "All" : s === "open" ? "Open" : "Closed",
      checked: (issueFilter.state ?? "open") === s,
      run: () => setIssueFilter("state", s),
    }));
  return (
    <>
      <div class="fg-filters">
        <div class="fg-search-row">
          <SearchBox value={issueFilter.search ?? ""} placeholder="Search issues" onChange={(v) => setIssueFilter("search", v || null)} />
        </div>
        <div class="fg-chips">
          <Chip label="State" value={issueFilter.state === "all" ? "All" : issueFilter.state === "closed" ? "Closed" : "Open"} onClick={(e) => menus.open(e, stateItems())} />
          <Chip
            label="Author"
            value={issueFilter.author === me() && me() ? "me" : issueFilter.author}
            onClick={(e) => menus.open(e, [{ label: "Me", checked: issueFilter.author === me(), run: () => setIssueFilter("author", me()) }, { label: "Any", checked: !issueFilter.author, run: () => setIssueFilter("author", null) }])}
            onClear={() => setIssueFilter("author", null)}
          />
          <Chip
            label="Assignee"
            value={issueFilter.assignee === me() && me() ? "me" : issueFilter.assignee}
            onClick={(e) => menus.open(e, [{ label: "Me", checked: issueFilter.assignee === me(), run: () => setIssueFilter("assignee", me()) }, { label: "Any", checked: !issueFilter.assignee, run: () => setIssueFilter("assignee", null) }])}
            onClear={() => setIssueFilter("assignee", null)}
          />
        </div>
      </div>
      <div class="fg-list">
        <Switch>
          <Match when={page.error}>
            <div class="fg-empty">
              <div class="fg-error-text">{errorText(page.error)}</div>
              <button class="btn-secondary" onClick={() => void refetch()}>
                Retry
              </button>
            </div>
          </Match>
          <Match when={page.loading && !page()}>
            <div class="fg-empty muted">Loading issues…</div>
          </Match>
          <Match when={!page()?.items.length}>
            <div class="fg-empty muted">No issues</div>
          </Match>
          <Match when={true}>
            <For each={page()!.items}>{(issue) => <IssueRow issue={issue} />}</For>
          </Match>
        </Switch>
      </div>
      {menus.view()}
    </>
  );
}

function IssueRow(props: { issue: Issue }) {
  const i = () => props.issue;
  return (
    <div
      class="fg-row"
      data-issue={i().number}
      title={`${i().title}${i().labels.length ? `\nLabels: ${i().labels.map((l) => l.name).join(", ")}` : ""}`}
      onClick={() => openIssue(i().number, i().title)}
    >
      <IssueStateIcon state={i().state} />
      <div class="fg-row-main">
        <div class="fg-row-title">
          <span class="fg-row-title-text">{i().title}</span>
        </div>
        <div class="fg-row-sub muted">
          <span class="fg-row-sub-text">
            #{i().number} opened by {displayName(i().author)} · updated {ago(i().updatedAt)}
          </span>
        </div>
      </div>
      <div class="fg-row-meta">
        <Show when={i().comments}>
          <span class="fg-comments">
            <MessageSquare size={13} />
            {i().comments}
          </span>
        </Show>
        <For each={i().assignees.slice(0, 2)}>{(u) => <Avatar user={u} size={18} />}</For>
      </div>
    </div>
  );
}

function PipelinesTab() {
  const [list, { refetch }] = createResource(() => (context()?.account ? revision() + 1 : false), () => forgesApi.pipelines());
  return (
    <div class="fg-list">
      <Switch>
        <Match when={list.error}>
          <div class="fg-empty">
            <div class="fg-error-text">{errorText(list.error)}</div>
            <button class="btn-secondary" onClick={() => void refetch()}>
              Retry
            </button>
          </div>
        </Match>
        <Match when={list.loading && !list()}>
          <div class="fg-empty muted">Loading pipelines…</div>
        </Match>
        <Match when={!list()?.length}>
          <div class="fg-empty muted">No pipelines</div>
        </Match>
        <Match when={true}>
          <For each={list()}>{(p) => <PipelineRow pipeline={p} />}</For>
        </Match>
      </Switch>
    </div>
  );
}

function PipelineRow(props: { pipeline: Pipeline }) {
  const p = () => props.pipeline;
  return (
    <div class="fg-row" data-pipeline={p().id} onClick={() => p().webUrl && window.open(p().webUrl!, "_blank")}>
      <CiIcon status={p().status} size={16} />
      <div class="fg-row-main">
        <div class="fg-row-title">
          <span class="fg-row-title-text">{p().name ?? `Pipeline ${p().id}`}</span>
        </div>
        <div class="fg-row-sub muted">
          {ciLabels[p().status]} · {p().ref ? <BranchPill name={p().ref!} /> : null} {p().sha?.slice(0, 8)} · {ago(p().createdAt)}
        </div>
      </div>
      <Show when={p().webUrl}>
        <span class="fg-row-meta muted">
          <ExternalLink size={13} />
        </span>
      </Show>
    </div>
  );
}

/** Header buttons of the tool window. */
export function PanelActions() {
  return (
    <>
      <button class="icon-btn" title="Refresh" onClick={() => void refreshAll()}>
        <RefreshCw size={14} />
      </button>
      <button class="icon-btn" title={`Create ${requestName()}…`} disabled={!context()?.account} onClick={openCreateDialog}>
        <Plus size={15} />
      </button>
    </>
  );
}

export function MergeRequestsPanel() {
  return (
    <div class="fg-panel">
      <div class="fg-tabs">
        <button class="git-tab" classList={{ active: tab() === "requests" }} onClick={() => setTab("requests")}>
          {requestName()}s
        </button>
        <button class="git-tab" classList={{ active: tab() === "issues" }} onClick={() => setTab("issues")}>
          Issues
        </button>
        <button class="git-tab" classList={{ active: tab() === "pipelines" }} onClick={() => setTab("pipelines")}>
          Pipelines
        </button>
      </div>
      <Show when={context()?.account} fallback={<NotConnected />}>
        <Switch>
          <Match when={tab() === "requests"}>
            <RequestsTab />
          </Match>
          <Match when={tab() === "issues"}>
            <IssuesTab />
          </Match>
          <Match when={tab() === "pipelines"}>
            <PipelinesTab />
          </Match>
        </Switch>
      </Show>
    </div>
  );
}
