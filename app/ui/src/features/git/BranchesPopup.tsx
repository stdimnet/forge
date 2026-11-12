/** Git branches popup from the toolbar branch widget (PHPStorm New UI layout). */
import { createEffect, createMemo, createResource, createSignal, For, on, onCleanup, onMount, Show, type JSX } from "solid-js";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Check,
  ChevronRight,
  Folder,
  GitBranch,
  GitBranchPlus,
  GitCommitHorizontal,
  Search,
  Tag,
  Star,
  Clock,
  Cloud,
  RefreshCw,
  FolderTree,
  Settings,
} from "lucide-solid";
import { TreeArrow } from "@/shell/TreeArrow";
import { saveSetting } from "@/state/settings";
import { navigate, type VisibleRow } from "@/features/project/treeNavigation";
import { flattenBranchTree, GROUP_BY_DIRECTORY, groupByDirectory, isFolder, toggled, type BranchTreeRow, type Expansion } from "./branchTree";
import { newWorktreeDialog, openWorktree } from "./extras";
import { gitApi, type BranchInfo, type BranchNode } from "./api";
import { revision, status } from "./store";
import { setBranchesOpen } from "./overlays";
import { showLogFor } from "./GitLogPanel";
import {
  checkout,
  checkoutRevisionDialog,
  deleteBranch,
  mergeIntoCurrent,
  newBranchDialog,
  openCommitWindow,
  pullInto,
  push,
  rebaseOnto,
  updateBranch,
  updateProject,
} from "./actions";
import { t as tr } from "@/i18n";

interface Action {
  id: string;
  label: string;
  hint?: string;
  icon: () => JSX.Element;
  run: () => void;
}

interface Row {
  key: string;
  branch: BranchInfo;
  remote: boolean;
  /** The name in its folder (`tabs`), or the full name in a flat list. */
  label: string;
  depth: number;
}

/** A row of the popup's tree, in display order: an action, a folder (or remote) or a branch. */
type Item = { type: "action"; key: string; action: Action } | { type: "folder"; key: string; row: BranchTreeRow } | ({ type: "branch" } & Row);

/** Folders the user opened or closed; kept while the window lives, like PHPStorm's popup. */
const [expansion, setExpansion] = createSignal<Expansion>(new Map());
/** Back to the folders' default state (tests). */
export const resetBranchFolders = (): void => void setExpansion(new Map());

/** Element id of a row (`aria-activedescendant`). */
const rowIds = new Map<string, string>();
const rowId = (key: string): string => {
  let id = rowIds.get(key);
  if (!id) rowIds.set(key, (id = `branches-row-${rowIds.size + 1}`));
  return id;
};

function anchorRect(): { left: number; top: number } {
  const btn = document.querySelector<HTMLElement>(`.main-toolbar [title="${tr("toolbar.gitBranches")}"]`);
  const r = btn?.getBoundingClientRect();
  return { left: Math.max(8, r?.left ?? 120), top: (r?.bottom ?? 40) + 4 };
}

export function BranchesPopup() {
  const [query, setQuery] = createSignal("");
  const [hovered, setHovered] = createSignal<{ row: Row; top: number } | null>(null);
  /** The row the arrow keys are on. */
  const [active, setActive] = createSignal<string | null>(null);
  /** Arrow keys were used since the last typing: Left and Right belong to the tree, not the caret. */
  const [navigated, setNavigated] = createSignal(false);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [branches] = createResource(revision, () => gitApi.branches());
  const pos = anchorRect();
  let input!: HTMLInputElement;
  let root!: HTMLDivElement;
  let gear!: HTMLButtonElement;
  const current = () => branches()?.current ?? status()?.branch ?? null;

  const match = (name: string) => name.toLowerCase().includes(query().trim().toLowerCase());
  const searching = () => query().trim() !== "";
  const grouped = groupByDirectory;

  /**
   * The rows of a section: the folders from Rust when grouping, a flat list of full names
   * otherwise. While searching, every folder with a matching branch is shown open.
   */
  const section = (name: "local" | "remote", list: BranchInfo[], tree: BranchNode[]): Item[] => {
    const remote = name === "remote";
    if (!grouped()) {
      return list.filter((b) => match(b.name)).map((b) => ({ type: "branch", key: `${name}:branch:${b.name}`, branch: b, remote, label: b.name, depth: 0 }));
    }
    const byName = new Map(list.map((b) => [b.name, b]));
    return flattenBranchTree(tree, { section: name, expansion: expansion(), match: searching() ? match : undefined }).flatMap((row): Item[] => {
      if (isFolder(row.node)) return [{ type: "folder", key: row.key, row }];
      const branch = byName.get(row.node.path);
      return branch ? [{ type: "branch", key: row.key, branch, remote, label: row.node.label, depth: row.depth }] : [];
    });
  };
  const local = createMemo(() => section("local", branches()?.local ?? [], branches()?.localTree ?? []));
  const remote = createMemo(() => section("remote", branches()?.remote ?? [], branches()?.remoteTree ?? []));
  // Recent stays a flat list of full names, like in PHPStorm.
  const recent = createMemo(() =>
    (branches()?.recent ?? [])
      .map((n) => branches()!.local.find((b) => b.name === n))
      .filter((b): b is BranchInfo => !!b && match(b.name))
      .map((b): Item => ({ type: "branch", key: `recent:branch:${b.name}`, branch: b, remote: false, label: b.name, depth: 0 })),
  );

  const actions: Action[] = [
    { id: "update", label: tr("git.branchesPopup.updateProject"), hint: "Ctrl+T", icon: () => <ArrowDownToLine size={15} />, run: () => (setBranchesOpen(false), void updateProject()) },
    { id: "commit", label: tr("git.branchesPopup.commit"), hint: "Ctrl+K", icon: () => <GitCommitHorizontal size={15} />, run: () => (setBranchesOpen(false), openCommitWindow()) },
    { id: "push", label: tr("git.branchesPopup.push"), hint: "Ctrl+Shift+K", icon: () => <ArrowUpFromLine size={15} />, run: () => (setBranchesOpen(false), void push()) },
    { id: "newBranch", label: tr("git.branchesPopup.newBranch"), hint: "Ctrl+Alt+N", icon: () => <GitBranchPlus size={15} />, run: () => newBranchDialog(null) },
    { id: "checkoutRevision", label: tr("git.branchesPopup.checkoutTagOrRevision"), icon: () => <Tag size={15} />, run: () => checkoutRevisionDialog() },
  ];
  const visibleActions = createMemo(() => actions.filter((a) => match(a.label)).map((a): Item => ({ type: "action", key: `action:${a.id}`, action: a })));

  // ---- keyboard: the search field keeps the focus, the arrow keys move through the rows ----
  const items = createMemo(() => [...visibleActions(), ...recent(), ...local(), ...remote()]);
  const navRows = createMemo(() =>
    items().map(
      (it): VisibleRow => ({
        path: it.key,
        isDir: it.type === "folder",
        expanded: it.type === "folder" && it.row.expanded,
        depth: it.type === "action" ? 0 : it.type === "folder" ? it.row.depth : it.depth,
      }),
    ),
  );
  const toggleFolder = (row: BranchTreeRow, open = !row.expanded) => setExpansion((e) => toggled(e, row, open));
  const openBranch = (row: Row) => {
    if (row.branch.current) return;
    if (row.branch.worktree) {
      setBranchesOpen(false);
      void openWorktree(row.branch.worktree);
    } else void checkout(row.branch.name, row.remote);
  };
  const activate = (it: Item) => {
    if (it.type === "action") it.action.run();
    else if (it.type === "folder") toggleFolder(it.row);
    else openBranch(it);
  };
  const select = (key: string | null) => {
    setActive(key);
    if (key) queueMicrotask(() => document.getElementById(rowId(key))?.scrollIntoView?.({ block: "nearest" }));
  };
  // Typing: Enter takes the first branch found (or the first action).
  createEffect(
    on([query, branches], () => {
      setNavigated(false);
      if (!searching()) return setActive((key) => (items().some((it) => it.key === key) ? key : null));
      select((items().find((it) => it.type === "branch") ?? items()[0])?.key ?? null);
    }),
  );
  const onInputKey = (e: KeyboardEvent) => {
    if (e.altKey || e.ctrlKey || e.metaKey || e.isComposing) return;
    const vertical = e.key === "ArrowDown" || e.key === "ArrowUp";
    const inTree = ["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key);
    if (!vertical && e.key !== "Enter" && !(inTree && navigated())) return;
    e.preventDefault();
    if (vertical) setNavigated(true);
    const action = navigate(navRows(), active(), e.key);
    const item = "path" in action ? items().find((it) => it.key === action.path) : undefined;
    if (action.type === "select") select(action.path);
    else if (item?.type === "folder" && (action.type === "expand" || action.type === "collapse")) toggleFolder(item.row, action.type === "expand");
    else if (item && action.type === "open") activate(item);
  };

  const toggleGrouping = () => void saveSetting(GROUP_BY_DIRECTORY, !grouped());
  const closeSettings = () => {
    setSettingsOpen(false);
    input.focus();
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      if (settingsOpen()) closeSettings();
      else setBranchesOpen(false);
    }
  };
  const onDown = (e: MouseEvent) => {
    const t = e.target as HTMLElement;
    if (!root.contains(t) && !t.closest(`[title="${tr("toolbar.gitBranches")}"]`) && !t.closest(".branch-submenu")) setBranchesOpen(false);
    else if (settingsOpen() && !t.closest(".branch-settings-menu") && !gear.contains(t)) setSettingsOpen(false);
  };
  onMount(() => {
    input.focus();
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onDown, true);
  });
  onCleanup(() => {
    window.removeEventListener("keydown", onKey, true);
    window.removeEventListener("mousedown", onDown, true);
  });

  /** Indent per tree level; a grouped list also keeps room for the folders' arrows. */
  const indent = (depth: number) => ({ "padding-left": `${8 + depth * 16}px` });
  const Twisty = (p: { row?: BranchTreeRow }) => (
    <Show when={grouped()}>
      <span class="tree-chevron" aria-hidden="true">
        <Show when={p.row}>{(row) => <TreeArrow open={row().expanded} />}</Show>
      </span>
    </Show>
  );

  const FolderRow = (p: { row: BranchTreeRow }) => (
    <div
      class="popup-row branch-folder"
      classList={{ active: active() === p.row.key }}
      style={indent(p.row.depth)}
      id={rowId(p.row.key)}
      role="treeitem"
      aria-level={p.row.depth + 1}
      aria-expanded={p.row.expanded}
      aria-selected={active() === p.row.key}
      data-folder={p.row.node.path}
      onMouseEnter={() => setHovered(null)}
      onClick={() => (setActive(p.row.key), toggleFolder(p.row))}
    >
      <Twisty row={p.row} />
      <span class="popup-icon">{p.row.node.kind === "remote" ? <Cloud size={14} /> : <Folder size={14} />}</span>
      <span class="popup-label">{p.row.node.label}</span>
    </div>
  );

  const BranchRow = (p: { row: Row }) => {
    const row = p.row;
    const b = row.branch;
    return (
      <div
        class="popup-row branch-row"
        classList={{ hovered: hovered()?.row.key === row.key, active: active() === row.key }}
        style={indent(row.depth)}
        id={rowId(row.key)}
        role="treeitem"
        aria-level={row.depth + 1}
        aria-selected={active() === row.key}
        aria-current={b.current ? "true" : undefined}
        title={row.label === b.name ? undefined : b.name}
        onMouseEnter={(e) => setHovered({ row, top: e.currentTarget.getBoundingClientRect().top })}
        onClick={() => openBranch(row)}
        data-branch={b.name}
        data-section={row.key.slice(0, row.key.indexOf(":"))}
      >
        <Twisty />
        <span class="popup-icon">
          <Show when={b.current} fallback={row.remote && !grouped() ? <Cloud size={14} /> : <GitBranch size={14} />}>
            <Star size={14} class="branch-current" fill="currentColor" />
          </Show>
        </span>
        <span class="popup-label">{row.label}</span>
        <Show when={b.ahead > 0}>
          <span class="branch-track outgoing" title={tr("tools.git.outgoingCommits", { ahead: b.ahead })}>
            ↑{b.ahead}
          </span>
        </Show>
        <Show when={b.behind > 0}>
          <span class="branch-track incoming" title={tr("tools.git.incomingCommits", { behind: b.behind })}>
            ↓{b.behind}
          </span>
        </Show>
        <Show when={b.worktree}>
          <span class="branch-worktree" title={tr("git.branchesPopup.checkedOutInTheWorktree", { worktree: b.worktree })} data-testid="branch-worktree">
            <FolderTree size={12} /> worktree
          </span>
        </Show>
        <Show when={b.upstream && !row.remote}>
          <span class="popup-hint">{b.upstream}</span>
        </Show>
        <ChevronRight size={14} class="muted" aria-hidden="true" />
      </div>
    );
  };

  const Rows = (p: { items: Item[] }) => (
    <For each={p.items}>{(it) => (it.type === "folder" ? <FolderRow row={it.row} /> : it.type === "branch" ? <BranchRow row={it} /> : null)}</For>
  );

  const Submenu = (props: { h: { row: Row; top: number } }) => {
    const h = props.h;
    const b = h.row.branch;
    const cur = current() ?? "HEAD";
    const items: ({ label: string; run: () => void; danger?: boolean } | null)[] = b.current
      ? [
          { label: tr("git.branchesPopup.newBranchFromName", { name: b.name }), run: () => newBranchDialog(b.name) },
          { label: tr("git.branchesPopup.push"), run: () => (setBranchesOpen(false), void push()) },
          { label: tr("git.branchesPopup.update"), run: () => (setBranchesOpen(false), void updateProject()) },
        ]
      : [
          b.worktree
            ? { label: tr("git.branchesPopup.openWorktree"), run: () => (setBranchesOpen(false), void openWorktree(b.worktree!)) }
            : { label: tr("git.branchesPopup.checkout"), run: () => void checkout(b.name, h.row.remote) },
          { label: tr("git.branchesPopup.newBranchFromName", { name: b.name }), run: () => newBranchDialog(b.name) },
          ...(h.row.remote || b.worktree ? [] : [{ label: tr("git.branchesPopup.newWorktree"), run: () => (setBranchesOpen(false), void newWorktreeDialog(b.name)) }]),
          null,
          { label: tr("git.branchesPopup.compareWithCur", { cur }), run: () => (setBranchesOpen(false), showLogFor(`${cur}..${b.name}`)) },
          null,
          { label: tr("git.branchesPopup.rebaseCurOntoName", { cur, name: b.name }), run: () => void rebaseOnto(b.name) },
          { label: tr("git.branchesPopup.mergeNameIntoCur", { name: b.name, cur }), run: () => void mergeIntoCurrent(b.name) },
          // A local branch that tracks a branch is fast-forwarded without checking it out.
          ...(!h.row.remote && !b.worktree && b.upstream ? [null, { label: tr("git.branchesPopup.update"), run: () => void updateBranch(b.name) }] : []),
          ...(h.row.remote
            ? [
                null,
                { label: tr("git.branchesPopup.pullIntoCurUsingRebase", { cur }), run: () => void pullInto(b.name, true) },
                { label: tr("git.branchesPopup.pullIntoCurUsingMerge", { cur }), run: () => void pullInto(b.name, false) },
              ]
            : []),
          ...(h.row.remote || b.worktree ? [] : [null, { label: tr("git.branchesPopup.delete"), run: () => deleteBranch(b.name), danger: true }]),
        ];
    const rect = root.getBoundingClientRect();
    return (
      <div class="popup branch-submenu" style={{ left: `${rect.right - 4}px`, top: `${h.top - 5}px` }}>
        <For each={items}>
          {(it) =>
            it ? (
              <div class="popup-row" classList={{ danger: it.danger }} onClick={() => it.run()}>
                <span class="popup-label">{it.label}</span>
              </div>
            ) : (
              <div class="popup-sep" />
            )
          }
        </For>
      </div>
    );
  };

  return (
    <>
      <div class="popup branches-popup" ref={root} style={{ left: `${pos.left}px`, top: `${pos.top}px` }}>
        <div class="popup-search">
          <Search size={14} class="muted" aria-hidden="true" />
          <input
            ref={input}
            placeholder={tr("git.branchesPopup.searchForBranchesAndActions")}
            aria-label={tr("git.branchesPopup.searchForBranchesAndActions")}
            role="combobox"
            aria-expanded="true"
            aria-controls="branches-popup-tree"
            aria-autocomplete="list"
            aria-activedescendant={active() ? rowId(active()!) : undefined}
            spellcheck={false}
            autocomplete="off"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={onInputKey}
          />
          <button
            ref={gear}
            class="icon-btn"
            classList={{ active: settingsOpen() }}
            title={tr("git.branchesPopup.settings")}
            aria-label={tr("git.branchesPopup.settings")}
            aria-haspopup="menu"
            aria-expanded={settingsOpen()}
            onClick={() => setSettingsOpen(!settingsOpen())}
          >
            <Settings size={14} />
          </button>
        </div>
        <Show when={settingsOpen()}>
          <div class="popup branch-settings-menu" role="menu" aria-label={tr("git.branchesPopup.settings")}>
            <button
              class="popup-row"
              role="menuitemcheckbox"
              aria-checked={grouped()}
              ref={(el) => queueMicrotask(() => el.focus())}
              onClick={toggleGrouping}
            >
              <span class="popup-icon">
                <Show when={grouped()}>
                  <Check size={14} />
                </Show>
              </span>
              <span class="popup-label">{tr("git.branchesPopup.groupByDirectory")}</span>
            </button>
          </div>
        </Show>
        {/* The focus stays in the search field; a click on a row must not take it away. */}
        <div class="popup-scroll" id="branches-popup-tree" role="tree" aria-label={tr("git.branchesPopup.branches")} onMouseDown={(e) => e.preventDefault()}>
          <For each={visibleActions()}>
            {(it, i) => (
              <Show when={it.type === "action" && it} keyed>
                {(a) => (
                  <>
                    <Show when={i() === 3}>
                      <div class="popup-sep" role="presentation" />
                    </Show>
                    <div
                      class="popup-row"
                      classList={{ active: active() === a.key }}
                      id={rowId(a.key)}
                      role="treeitem"
                      aria-level={1}
                      aria-selected={active() === a.key}
                      onClick={a.action.run}
                      onMouseEnter={() => setHovered(null)}
                    >
                      <span class="popup-icon">{a.action.icon()}</span>
                      <span class="popup-label">{a.action.label}</span>
                      <Show when={a.action.hint}>
                        <span class="popup-hint">{a.action.hint}</span>
                      </Show>
                    </div>
                  </>
                )}
              </Show>
            )}
          </For>
          <Show when={recent().length}>
            <div class="popup-section" role="presentation">
              <Clock size={12} /> {tr("git.branchesPopup.recent")}
            </div>
            <Rows items={recent()} />
          </Show>
          <Show when={local().length}>
            <div class="popup-section" role="presentation">
              {tr("git.branchesPopup.local")}
            </div>
            <Rows items={local()} />
          </Show>
          <Show when={remote().length}>
            <div class="popup-section" role="presentation">
              {tr("git.branchesPopup.remote")}
            </div>
            <Rows items={remote()} />
          </Show>
          <Show when={branches.loading}>
            <div class="popup-section" role="presentation">
              <RefreshCw size={12} class="spin" /> {tr("git.branchesPopup.loading")}
            </div>
          </Show>
        </div>
      </div>
      <Show when={hovered()} keyed>
        {(h) => <Submenu h={h} />}
      </Show>
    </>
  );
}
