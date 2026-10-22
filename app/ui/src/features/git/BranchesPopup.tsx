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
} from "lucide-solid";
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
  updateProject,
} from "./actions";

interface Row {
  key: string;
  branch: BranchInfo;
  remote: boolean;
}

function anchorRect(): { left: number; top: number } {
  const btn = document.querySelector<HTMLElement>('.main-toolbar [title="Git Branches"]');
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
    { label: "Update Project…", hint: "Ctrl+T", icon: () => <ArrowDownToLine size={15} />, run: () => (setBranchesOpen(false), void updateProject()) },
    { label: "Commit…", hint: "Ctrl+K", icon: () => <GitCommitHorizontal size={15} />, run: () => (setBranchesOpen(false), openCommitWindow()) },
    { label: "Push…", hint: "Ctrl+Shift+K", icon: () => <ArrowUpFromLine size={15} />, run: () => (setBranchesOpen(false), void push()) },
    { label: "New Branch…", hint: "Ctrl+Alt+N", icon: () => <GitBranchPlus size={15} />, run: () => newBranchDialog(null) },
    { label: "Checkout Tag or Revision…", icon: () => <Tag size={15} />, run: () => checkoutRevisionDialog() },
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
    if (!root.contains(t) && !t.closest('[title="Git Branches"]') && !t.closest(".branch-submenu")) setBranchesOpen(false);
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
        onClick={() => !p.b.current && void checkout(p.b.name, p.remote)}
      >
        <span class="popup-icon">
          <Show when={p.b.current} fallback={p.remote ? <Cloud size={14} /> : <GitBranch size={14} />}>
            <Star size={14} class="branch-current" fill="currentColor" />
          </Show>
        </span>
        <span class="popup-label">{p.b.name}</span>
        <Show when={p.b.ahead > 0}>
          <span class="branch-track outgoing" title={`${p.b.ahead} outgoing commits`}>
            ↑{p.b.ahead}
          </span>
        </Show>
        <Show when={p.b.behind > 0}>
          <span class="branch-track incoming" title={`${p.b.behind} incoming commits`}>
            ↓{p.b.behind}
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
          { label: `New Branch from '${b.name}'…`, run: () => newBranchDialog(b.name) },
          { label: "Push…", run: () => (setBranchesOpen(false), void push()) },
          { label: "Update", run: () => (setBranchesOpen(false), void updateProject()) },
        ]
      : [
          { label: "Checkout", run: () => void checkout(b.name, h.row.remote) },
          { label: `New Branch from '${b.name}'…`, run: () => newBranchDialog(b.name) },
          null,
          { label: `Compare with '${cur}'`, run: () => (setBranchesOpen(false), showLogFor(`${cur}..${b.name}`)) },
          null,
          { label: `Rebase '${cur}' onto '${b.name}'`, run: () => void rebaseOnto(b.name) },
          { label: `Merge '${b.name}' into '${cur}'`, run: () => void mergeIntoCurrent(b.name) },
          ...(h.row.remote ? [] : [null, { label: "Delete", run: () => deleteBranch(b.name), danger: true }]),
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
          <input ref={input} placeholder="Search for branches and actions" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
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
              <Clock size={12} /> Recent
            </div>
            <For each={recent()}>{(b) => <BranchRow b={b} section="recent" />}</For>
          </Show>
          <Show when={local().length}>
            <div class="popup-section">Local</div>
            <For each={local()}>{(b) => <BranchRow b={b} section="local" />}</For>
          </Show>
          <Show when={remote().length}>
            <div class="popup-section">Remote</div>
            <For each={remote()}>{(b) => <BranchRow b={b} remote section="remote" />}</For>
          </Show>
          <Show when={branches.loading}>
            <div class="popup-section">
              <RefreshCw size={12} class="spin" /> Loading…
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
