/**
 * The Profiler tool window (PyCharm's cProfile snapshot view): the call tree with total
 * and own time per call path, and the method list of hot spots (own time, total time,
 * calls). Double-click opens the function's source.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, FileInput, History, Trash2 } from "lucide-solid";
import { openMenu } from "@/features/run/PopupMenu";
import { openFileAt } from "@/editor/views";
import { askName } from "@/features/contextMenu/dialogs";
import type { CallNode, HotSpot } from "@/ipc/pythonTools";
import { currentSnapshot, deleteSnapshot, openSnapshotFile, profileLoading, showSnapshot, snapshots } from "./store";
import { t } from "@/i18n";

const when = (at: number) => new Date(at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
const ms = (v: number) => (v >= 1000 ? `${(v / 1000).toFixed(2)} s` : `${v.toFixed(v < 10 ? 2 : 1)} ms`);

const navigable = (file: string) => !!file && file !== "~" && !file.startsWith("<");

function openSource(file: string, line: number): void {
  if (navigable(file)) void openFileAt(file, Math.max(1, line), 1);
}

export function showSnapshotsMenu(x: number, y: number): void {
  const list = snapshots();
  openMenu({
    x,
    y,
    items: list.length
      ? list.map((s) => ({ label: s.name, shortcut: when(s.at), run: () => void showSnapshot(s.id) }))
      : [{ label: t("langs.python.noSnapshotsYet"), disabled: true }],
  });
}

async function openFile(): Promise<void> {
  const path = await askName({ title: t("langs.python.openProfilerSnapshot"), label: t("langs.python.projectRelativePathOfALabel"), initial: "", confirm: t("common.open") });
  if (path?.trim()) await openSnapshotFile(path.trim());
}

export function ProfilerActions() {
  return (
    <>
      <button
        class="icon-btn"
        title={t("langs.python.snapshotsEllipsis")}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          showSnapshotsMenu(r.left, r.bottom + 2);
        }}
      >
        <History size={14} />
      </button>
      <button class="icon-btn" title={t("langs.python.openSnapshotEllipsis")} onClick={() => void openFile()}>
        <FileInput size={14} />
      </button>
      <button class="icon-btn" title={t("langs.python.deleteSnapshot")} disabled={!currentSnapshot()} onClick={() => currentSnapshot() && void deleteSnapshot(currentSnapshot()!.info.id)}>
        <Trash2 size={14} />
      </button>
    </>
  );
}

interface Row {
  node: CallNode;
  key: string;
  depth: number;
}

function Bar(props: { percent: number }) {
  return (
    <span class="prof-bar" title={`${props.percent}%`}>
      <span class="prof-bar-fill" style={{ width: `${Math.min(100, props.percent)}%` }} />
    </span>
  );
}

function CallTree() {
  // Expanded paths; the first levels start open.
  const [open, setOpen] = createSignal<Set<string>>(new Set(), { equals: false });
  const [selected, setSelected] = createSignal<string | null>(null);
  const [touched, setTouched] = createSignal(false);
  const isOpen = (key: string, depth: number) => (touched() ? open().has(key) : depth < 3 || open().has(key));
  const rows = createMemo(() => {
    const out: Row[] = [];
    const walk = (nodes: CallNode[], depth: number, prefix: string) => {
      nodes.forEach((n, i) => {
        const key = `${prefix}/${i}`;
        out.push({ node: n, key, depth });
        if (n.children?.length && isOpen(key, depth)) walk(n.children, depth + 1, key);
      });
    };
    walk(currentSnapshot()?.view.callTree ?? [], 0, "");
    return out;
  });
  const toggle = (r: Row) => {
    if (!touched()) {
      // Keep what is open now, then flip this row.
      const now = new Set<string>();
      for (const x of rows()) if (x.node.children?.length && isOpen(x.key, x.depth)) now.add(x.key);
      setOpen(now);
      setTouched(true);
    }
    const s = open();
    if (s.has(r.key)) s.delete(r.key);
    else s.add(r.key);
    setOpen(s);
  };
  return (
    <div class="prof-table" role="treegrid" data-testid="profiler-call-tree">
      <div class="prof-head prof-tree-cols">
        <span>{t("common.name")}</span>
        <span>{t("langs.python.calls")}</span>
        <span>{t("langs.python.totalTime")}</span>
        <span>{t("langs.python.ownTime")}</span>
        <span>{t("langs.python.time")}</span>
      </div>
      <div class="prof-body">
        <For each={rows()}>
          {(r) => (
            <div
              class="prof-row prof-tree-cols"
              classList={{ selected: selected() === r.key }}
              onClick={() => setSelected(r.key)}
              onDblClick={() => openSource(r.node.file, r.node.line)}
              title={navigable(r.node.file) ? `${r.node.file}:${r.node.line}` : r.node.name}
            >
              <span class="prof-name" style={{ "padding-left": `${4 + r.depth * 14}px` }}>
                <span class="prof-chevron" onClick={(e) => (e.stopPropagation(), r.node.children?.length && toggle(r))}>
                  <Show when={r.node.children?.length}>{isOpen(r.key, r.depth) ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
                </span>
                <span classList={{ "prof-fn": true, builtin: !navigable(r.node.file) }}>{r.node.name}</span>
                <Show when={navigable(r.node.file)}>
                  <span class="muted prof-loc">
                    {r.node.file.split("/").pop()}:{r.node.line}
                  </span>
                </Show>
              </span>
              <span class="prof-num">{r.node.calls}</span>
              <span class="prof-num">{ms(r.node.totalMs)}</span>
              <span class="prof-num">{ms(r.node.ownMs)}</span>
              <span class="prof-pct">
                <Bar percent={r.node.totalPercent} />
                {r.node.totalPercent.toFixed(1)}%
              </span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

type SortKey = "ownMs" | "totalMs" | "calls" | "name";

function HotSpots() {
  const [sort, setSort] = createSignal<SortKey>("ownMs");
  const [selected, setSelected] = createSignal<number | null>(null);
  const rows = createMemo(() => {
    const list = [...(currentSnapshot()?.view.hotSpots ?? [])];
    const k = sort();
    return list.sort((a: HotSpot, b: HotSpot) => (k === "name" ? a.name.localeCompare(b.name) : (b[k] as number) - (a[k] as number)));
  });
  const head = (key: SortKey, label: string) => (
    <span class="prof-sort" classList={{ active: sort() === key }} onClick={() => setSort(key)}>
      {label}
      {sort() === key && key !== "name" ? " ↓" : ""}
    </span>
  );
  return (
    <div class="prof-table" role="grid" data-testid="profiler-hot-spots">
      <div class="prof-head prof-list-cols">
        {head("name", "Name")}
        {head("calls", "Calls")}
        {head("ownMs", "Own Time")}
        <span>{t("langs.python.own")}</span>
        {head("totalMs", "Total Time")}
      </div>
      <div class="prof-body">
        <For each={rows()}>
          {(h) => (
            <div
              class="prof-row prof-list-cols"
              classList={{ selected: selected() === h.function }}
              onClick={() => setSelected(h.function)}
              onDblClick={() => openSource(h.file, h.line)}
            >
              <span class="prof-name">
                <span classList={{ "prof-fn": true, builtin: !navigable(h.file) }}>{h.name}</span>
                <Show when={navigable(h.file)}>
                  <span class="muted prof-loc">
                    {h.file}:{h.line}
                  </span>
                </Show>
              </span>
              <span class="prof-num">{h.calls === h.primitiveCalls ? h.calls : `${h.calls}/${h.primitiveCalls}`}</span>
              <span class="prof-num">{ms(h.ownMs)}</span>
              <span class="prof-pct">
                <Bar percent={h.ownPercent} />
                {h.ownPercent.toFixed(1)}%
              </span>
              <span class="prof-num">{ms(h.totalMs)}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

export function ProfilerView() {
  const [tab, setTab] = createSignal<"tree" | "list">("tree");
  return (
    <div class="prof-window" data-testid="profiler">
      <Show
        when={currentSnapshot()}
        fallback={
          <div class="prof-empty muted">
            <Show when={!profileLoading()} fallback={<p>{t("langs.python.readingTheSnapshotEllipsis")}</p>}>
              <p>{t("langs.python.noProfilerSnapshot")}</p>
              <p>{t("langs.python.runAPythonConfigurationWith")}</p>
            </Show>
          </div>
        }
      >
        {(s) => (
          <>
            <div class="prof-toolbar">
              <div class="prof-tabs" role="tablist">
                <button role="tab" classList={{ "prof-tab": true, active: tab() === "tree" }} onClick={() => setTab("tree")}>
                  {t("common.callTree")}
                </button>
                <button role="tab" classList={{ "prof-tab": true, active: tab() === "list" }} onClick={() => setTab("list")}>
                  {t("langs.python.methodList")}
                </button>
              </div>
              <span class="prof-summary muted">
                <b>{s().info.name}</b> {t("langs.python.callsFunctions", { value: when(s().info.at), value2: ms(s().view.totalMs), calls: s().view.calls.toLocaleString(), functions: s().view.functions })}
              </span>
            </div>
            <Show when={tab() === "tree"} fallback={<HotSpots />}>
              <CallTree />
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}
