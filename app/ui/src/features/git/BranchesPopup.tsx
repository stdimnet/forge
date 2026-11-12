/** Git branches popup from the toolbar branch widget (PHPStorm New UI layout). */
import { createMemo, createResource, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  ChevronRight,
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
} from "lucide-solid";
import { newWorktreeDialog, openWorktree } from "./extras";
import { gitApi, type BranchInfo } from "./api";
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
  push,
  rebaseOnto,
  updateBranch,
  updateProject,
} from "./actions";
import { t as tr } from "@/i18n";

interface Row {
  key: string;
  branch: BranchInfo;
  remote: boolean;
}

function anchorRect(): { left: number; top: number } {
  const btn = document.querySelector<HTMLElement>(`.main-toolbar [title="${tr("toolbar.gitBranches")}"]`);
  const r = btn?.getBoundingClientRect();
  return { left: Math.max(8, r?.left ?? 120), top: (r?.bottom ?? 40) + 4 };
}

export function BranchesPopup() {
  const [query, setQuery] = createSignal("");
  const [hovered, setHovered] = createSignal<{ row: Row; top: number } | null>(null);
  const [branches] = createResource(revision, () => gitApi.branches());
  const pos = anchorRect();
  let input!: HTMLInputElement;
  let root!: HTMLDivElement;
  const current = () => branches()?.current ?? status()?.branch ?? null;

  const match = (name: string) => name.toLowerCase().includes(query().trim().toLowerCase());
  const local = createMemo(() => (branches()?.local ?? []).filter((b) => match(b.name)));
  const remote = createMemo(() => (branches()?.remote ?? []).filter((b) => match(b.name)));
  const recent = createMemo(() =>
    (branches()?.recent ?? [])
      .map((n) => branches()!.local.find((b) => b.name === n))
      .filter((b): b is BranchInfo => !!b && match(b.name)),
  );

  const actions: { label: string; hint?: string; icon: () => JSX.Element; run: () => void }[] = [
    { label: tr("git.branchesPopup.updateProject"), hint: "Ctrl+T", icon: () => <ArrowDownToLine size={15} />, run: () => (setBranchesOpen(false), void updateProject()) },
    { label: tr("git.branchesPopup.commit"), hint: "Ctrl+K", icon: () => <GitCommitHorizontal size={15} />, run: () => (setBranchesOpen(false), openCommitWindow()) },
    { label: tr("git.branchesPopup.push"), hint: "Ctrl+Shift+K", icon: () => <ArrowUpFromLine size={15} />, run: () => (setBranchesOpen(false), void push()) },
    { label: tr("git.branchesPopup.newBranch"), hint: "Ctrl+Alt+N", icon: () => <GitBranchPlus size={15} />, run: () => newBranchDialog(null) },
    { label: tr("git.branchesPopup.checkoutTagOrRevision"), icon: () => <Tag size={15} />, run: () => checkoutRevisionDialog() },
  ];
  const visibleActions = () => actions.filter((a) => match(a.label));

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      setBranchesOpen(false);
    }
  };
  const onDown = (e: MouseEvent) => {
    const t = e.target as HTMLElement;
    if (!root.contains(t) && !t.closest(`[title="${tr("toolbar.gitBranches")}"]`) && !t.closest(".branch-submenu")) setBranchesOpen(false);
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

  const BranchRow = (p: { b: BranchInfo; remote?: boolean; section: string }) => {
    const row: Row = { key: `${p.section}:${p.b.name}`, branch: p.b, remote: !!p.remote };
    return (
      <div
        class="popup-row branch-row"
        classList={{ hovered: hovered()?.row.key === row.key }}
        onMouseEnter={(e) => setHovered({ row, top: e.currentTarget.getBoundingClientRect().top })}
        onClick={() => !p.b.current && (p.b.worktree ? (setBranchesOpen(false), void openWorktree(p.b.worktree)) : void checkout(p.b.name, p.remote))}
        data-branch={p.b.name}
      >
        <span class="popup-icon">
          <Show when={p.b.current} fallback={p.remote ? <Cloud size={14} /> : <GitBranch size={14} />}>
            <Star size={14} class="branch-current" fill="currentColor" />
          </Show>
        </span>
        <span class="popup-label">{p.b.name}</span>
        <Show when={p.b.ahead > 0}>
          <span class="branch-track outgoing" title={tr("tools.git.outgoingCommits", { ahead: p.b.ahead })}>
            ↑{p.b.ahead}
          </span>
        </Show>
        <Show when={p.b.behind > 0}>
          <span class="branch-track incoming" title={tr("tools.git.incomingCommits", { behind: p.b.behind })}>
            ↓{p.b.behind}
          </span>
        </Show>
        <Show when={p.b.worktree}>
          <span class="branch-worktree" title={tr("git.branchesPopup.checkedOutInTheWorktree", { worktree: p.b.worktree })} data-testid="branch-worktree">
            <FolderTree size={12} /> worktree
          </span>
        </Show>
        <Show when={p.b.upstream && !p.remote}>
          <span class="popup-hint">{p.b.upstream}</span>
        </Show>
        <ChevronRight size={14} class="muted" />
      </div>
    );
  };

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
        <label class="popup-search">
          <Search size={14} class="muted" />
          <input ref={input} placeholder={tr("git.branchesPopup.searchForBranchesAndActions")} value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
        </label>
        <div class="popup-scroll">
          <For each={visibleActions()}>
            {(a, i) => (
              <>
                <Show when={i() === 3}>
                  <div class="popup-sep" />
                </Show>
                <div class="popup-row" onClick={a.run} onMouseEnter={() => setHovered(null)}>
                  <span class="popup-icon">{a.icon()}</span>
                  <span class="popup-label">{a.label}</span>
                  <Show when={a.hint}>
                    <span class="popup-hint">{a.hint}</span>
                  </Show>
                </div>
              </>
            )}
          </For>
          <Show when={recent().length}>
            <div class="popup-section">
              <Clock size={12} /> {tr("git.branchesPopup.recent")}
            </div>
            <For each={recent()}>{(b) => <BranchRow b={b} section="recent" />}</For>
          </Show>
          <Show when={local().length}>
            <div class="popup-section">{tr("git.branchesPopup.local")}</div>
            <For each={local()}>{(b) => <BranchRow b={b} section="local" />}</For>
          </Show>
          <Show when={remote().length}>
            <div class="popup-section">{tr("git.branchesPopup.remote")}</div>
            <For each={remote()}>{(b) => <BranchRow b={b} remote section="remote" />}</For>
          </Show>
          <Show when={branches.loading}>
            <div class="popup-section">
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
