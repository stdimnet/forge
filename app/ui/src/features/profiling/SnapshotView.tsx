/**
 * A profiler snapshot in the Profiler tool window, laid out like PHPStorm's Xdebug snapshot
 * view: Execution Statistics (callable, calls, own and total cost with percentages; the
 * selected callable's callees and callers below), the Call Tree and the Flame Graph of the
 * shared profiler views. Heap snapshots get the Memory view instead: DevTools' Summary by
 * constructor and the biggest objects. Everything is computed in Rust (`forge-profiler`);
 * this module only draws it. Double-click opens a project function's source.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { ChevronDown, ChevronRight, Flame, HardDrive, ListTree, Rows3, Table2 } from "lucide-solid";
import type { FunctionStats, HeapClass, HeapSummary, Snapshot } from "@/ipc/profiler";
import { openFileAt } from "@/editor/views";
import { formatValue, hashColor, percent, ProfileBody, type FrameStyle } from "@/features/profiler/callTree";
import { workspace } from "@/state/workspace";
import { loadedSnapshot, profilerBusy, SOURCE_LABELS, switchMetric, type SourceId } from "./store";
import "./profiling.css";

type Tab = "stats" | "tree" | "flame" | "summary" | "biggest";

const [tabs, setTabs] = createStore<Partial<Record<SourceId, Tab>>>({});

const root = () => (workspace()?.root ?? "").replace(/\\/g, "/");

/** Whether a frame's file is a project file (navigable, highlighted). */
export function isOwn(file: string): boolean {
  if (!file || file === "php:internal" || file.startsWith("node:") || file.startsWith("(")) return false;
  const r = root();
  if (/(^|\/)(vendor|node_modules)\//.test(file)) return false;
  if (/^(\/|~|[A-Za-z]:\/)/.test(file)) return !!r && file.startsWith(`${r}/`);
  return !/^[a-z]+:/i.test(file);
}

export function navigate(file: string, line: number): void {
  const r = root();
  void openFileAt(file.startsWith(`${r}/`) ? file.slice(r.length + 1) : file, Math.max(1, line));
}

/** `App\Services\PriceCalculator->total` → `PriceCalculator->total`. */
const shortName = (name: string) => name.replace(/^(?:[A-Za-z_][\w]*\\)+(?=[A-Za-z_])/, "");

function colorOf(name: string): string {
  if (name.startsWith("php::") || name.startsWith("(") || /^(PDO|mysqli|SplObject)/.test(name)) return "var(--graph-5)";
  if (/^(Illuminate|Symfony|Composer|Doctrine|GuzzleHttp|require|include)/.test(name)) return "var(--graph-3)";
  const cls = /^(?:[\w\\]+\\)?(\w+)(?:->|::)/.exec(name)?.[1] ?? name.replace(/\(.*$/, "");
  return hashColor(cls);
}

const style: FrameStyle = { navigate, isOwn, shortName, colorOf };

/** The noun of a metric in column headings: Time, Memory, Samples, … */
function noun(s: Snapshot): string {
  const m = s.metrics[s.metric];
  if (!m) return "Cost";
  if (m.unit === "nanoseconds") return "Time";
  if (m.unit === "bytes") return s.kind === "v8-heap-sampling" ? "Size" : "Memory";
  return m.kind;
}

const unit = (s: Snapshot) => s.metrics[s.metric]?.unit ?? "";

function where(f: { file: string; line: number }): string {
  if (!f.file) return "";
  const r = root();
  const rel = f.file.startsWith(`${r}/`) ? f.file.slice(r.length + 1) : f.file;
  return f.line ? `${rel}:${f.line}` : rel;
}

type SortKey = "name" | "calls" | "own" | "total";
const MAX_ROWS = 500;

function Callable(props: { f: FunctionStats }) {
  return (
    <span class="profiling-callable" classList={{ "prof-own": isOwn(props.f.file) }} title={[props.f.name, where(props.f)].filter(Boolean).join("\n")}>
      {props.f.name}
    </span>
  );
}

function ExecutionStatistics(props: { snapshot: Snapshot }) {
  const [sort, setSort] = createSignal<SortKey>("own");
  const [filter, setFilter] = createSignal("");
  const [selected, setSelected] = createSignal<number | null>(null);
  const s = () => props.snapshot;
  const rows = createMemo(() => {
    const q = filter().trim().toLowerCase();
    const k = sort();
    const list = s()
      .functions.map((f, i) => ({ f, i }))
      .filter((r) => !q || r.f.name.toLowerCase().includes(q));
    list.sort((a, b) => (k === "name" ? a.f.name.localeCompare(b.f.name) : b.f[k] - a.f[k] || a.f.name.localeCompare(b.f.name)));
    return list;
  });
  const head = (key: SortKey, label: string, cls = "") => (
    <th class={cls} classList={{ "profiling-sorted": sort() === key }} onClick={() => setSort(key)}>
      {label}
      {sort() === key && key !== "name" ? " ↓" : ""}
    </th>
  );
  const callees = createMemo(() => {
    const i = selected();
    return i === null ? [] : s().edges.filter((e) => e.caller === i).sort((a, b) => b.total - a.total);
  });
  const callers = createMemo(() => {
    const i = selected();
    return i === null ? [] : s().edges.filter((e) => e.callee === i).sort((a, b) => b.total - a.total);
  });
  const open = (f: FunctionStats) => isOwn(f.file) && navigate(f.file, f.line);
  const EdgeList = (p: { title: string; edges: { fn: number; calls: number; total: number }[] }) => (
    <div class="profiling-edges">
      <div class="profiling-edges-title">
        {p.title} <span class="muted">{p.edges.length}</span>
      </div>
      <div class="profiling-scroll">
        <table class="prof-top profiling-table">
          <thead>
            <tr>
              <th>Callable</th>
              <th>{s().callsLabel}</th>
              <th>{noun(s())}</th>
              <th>%</th>
            </tr>
          </thead>
          <tbody>
            <For each={p.edges}>
              {(e) => (
                <tr onClick={() => setSelected(e.fn)} onDblClick={() => open(s().functions[e.fn])}>
                  <td>
                    <Callable f={s().functions[e.fn]} />
                  </td>
                  <td>{e.calls.toLocaleString("en-US")}</td>
                  <td>{formatValue(e.total, unit(s()))}</td>
                  <td>{percent(e.total, s().total)}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </div>
  );
  return (
    <div class="profiling-stats" data-testid="profiler-statistics">
      <div class="profiling-filter">
        <input class="input" placeholder="Filter callables" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
        <span class="muted">
          {rows().length.toLocaleString("en-US")} callables{rows().length > MAX_ROWS ? `, the first ${MAX_ROWS} shown` : ""}
        </span>
      </div>
      <div class="profiling-scroll profiling-main">
        <table class="prof-top profiling-table">
          <thead>
            <tr>
              {head("name", "Callable")}
              {head("calls", s().callsLabel === "Calls" ? "Invocation Count" : s().callsLabel)}
              {head("own", `Own ${noun(s())}`)}
              <th>Own %</th>
              {head("total", noun(s()))}
              <th>%</th>
            </tr>
          </thead>
          <tbody>
            <For each={rows().slice(0, MAX_ROWS)}>
              {(r) => (
                <tr classList={{ "profiling-selected": selected() === r.i }} onClick={() => setSelected(r.i)} onDblClick={() => open(r.f)}>
                  <td>
                    <Callable f={r.f} />
                    <Show when={where(r.f) && isOwn(r.f.file)}>
                      <span class="muted profiling-where">{where(r.f)}</span>
                    </Show>
                  </td>
                  <td>{r.f.calls.toLocaleString("en-US")}</td>
                  <td>{formatValue(r.f.own, unit(s()))}</td>
                  <td>
                    <span class="profiling-pct">
                      <span class="prof-bar">
                        <span style={{ width: percent(Math.abs(r.f.own), Math.abs(s().total)) }} />
                      </span>
                      {percent(r.f.own, s().total)}
                    </span>
                  </td>
                  <td>{formatValue(r.f.total, unit(s()))}</td>
                  <td>{percent(r.f.total, s().total)}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
      <Show when={selected() !== null}>
        <div class="profiling-split">
          <EdgeList title="Callees" edges={callees().map((e) => ({ fn: e.callee, calls: e.calls, total: e.total }))} />
          <EdgeList title="Callers" edges={callers().map((e) => ({ fn: e.caller, calls: e.calls, total: e.total }))} />
        </div>
      </Show>
    </div>
  );
}

function ClassRow(props: { c: HeapClass; heap: HeapSummary }) {
  const [open, setOpen] = createSignal(false);
  return (
    <>
      <tr onClick={() => setOpen(!open())}>
        <td>
          <span class="profiling-toggle">{open() ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</span>
          <span class="profiling-callable">{props.c.name}</span>
        </td>
        <td>{props.c.count.toLocaleString("en-US")}</td>
        <td>{formatValue(props.c.shallow, "bytes")}</td>
        <td>{percent(props.c.shallow, props.heap.totalSize)}</td>
        <td>{formatValue(props.c.retained, "bytes")}</td>
        <td>
          <span class="profiling-pct">
            <span class="prof-bar">
              <span style={{ width: percent(props.c.retained, props.heap.totalSize) }} />
            </span>
            {percent(props.c.retained, props.heap.totalSize)}
          </span>
        </td>
      </tr>
      <Show when={open()}>
        <For each={props.c.objects}>
          {(o) => (
            <tr class="profiling-object">
              <td>
                <span class="profiling-callable">{o.name}</span> <span class="muted">@{o.id}</span>
              </td>
              <td />
              <td>{formatValue(o.shallow, "bytes")}</td>
              <td>{percent(o.shallow, props.heap.totalSize)}</td>
              <td>{formatValue(o.retained, "bytes")}</td>
              <td>{percent(o.retained, props.heap.totalSize)}</td>
            </tr>
          )}
        </For>
        <Show when={props.c.count > props.c.objects.length}>
          <tr class="profiling-object">
            <td class="muted">{(props.c.count - props.c.objects.length).toLocaleString("en-US")} smaller objects</td>
            <td />
            <td />
            <td />
            <td />
            <td />
          </tr>
        </Show>
      </Show>
    </>
  );
}

function MemoryView(props: { heap: HeapSummary; tab: Tab }) {
  return (
    <div class="profiling-scroll profiling-main" data-testid="profiler-memory">
      <Show
        when={props.tab === "biggest"}
        fallback={
          <table class="prof-top profiling-table">
            <thead>
              <tr>
                <th>Constructor</th>
                <th>Count</th>
                <th>Shallow Size</th>
                <th>%</th>
                <th>Retained Size</th>
                <th>%</th>
              </tr>
            </thead>
            <tbody>
              <For each={props.heap.classes}>{(c) => <ClassRow c={c} heap={props.heap} />}</For>
            </tbody>
          </table>
        }
      >
        <table class="prof-top profiling-table">
          <thead>
            <tr>
              <th>Object</th>
              <th>Constructor</th>
              <th>Shallow Size</th>
              <th>Retained Size</th>
              <th>%</th>
            </tr>
          </thead>
          <tbody>
            <For each={props.heap.biggest}>
              {(o) => (
                <tr>
                  <td>
                    <span class="profiling-callable">{o.name}</span> <span class="muted">@{o.id}</span>
                  </td>
                  <td class="profiling-left">{o.className}</td>
                  <td>{formatValue(o.shallow, "bytes")}</td>
                  <td>{formatValue(o.retained, "bytes")}</td>
                  <td>{percent(o.retained, props.heap.totalSize)}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </div>
  );
}

const EMPTY: Record<SourceId, string> = {
  xdebug: "Run a PHP configuration with Run | Profile with Xdebug, or open a cachegrind.out file with Tools | Analyze Xdebug Profiler Snapshot.",
  spx: "Run a PHP configuration with Run | Profile with SPX (php-spx), or open an SPX report with Tools | Open Profiler Snapshot.",
  "v8-cpu": "Run a Node.js or npm configuration with Run | Profile with V8, or open a .cpuprofile file with Tools | Open Profiler Snapshot.",
  "v8-heap": "Run a Node.js or npm configuration with Run | Profile Allocations with V8, or open a .heapsnapshot or .heapprofile file with Tools | Open Profiler Snapshot.",
};

function TabButton(props: { source: SourceId; tab: Tab; current: Tab; label: string; icon: typeof Flame }) {
  return (
    <button classList={{ active: props.current === props.tab }} onClick={() => setTabs(props.source, props.tab)}>
      <props.icon size={13} /> {props.label}
    </button>
  );
}

function summary(s: Snapshot): string {
  if (s.heap) {
    const h = s.heap;
    return `${formatValue(h.totalSize, "bytes")} in ${h.objectCount.toLocaleString("en-US")} objects${h.unreachableSize ? ` · ${formatValue(h.unreachableSize, "bytes")} unreachable` : ""}`;
  }
  const parts = [`Total ${formatValue(s.total, unit(s))}`, `${s.functions.length.toLocaleString("en-US")} callables`];
  const d = s.view?.durationNanos;
  if (d && unit(s) !== "nanoseconds") parts.push(`${formatValue(d, "nanoseconds")} recorded`);
  return parts.join(" · ");
}

/** The view of one source (Xdebug, SPX, V8 CPU, V8 Heap). */
export function SnapshotView(props: { source: SourceId }) {
  const l = () => loadedSnapshot(props.source);
  const tab = (): Tab => {
    const s = l()?.snapshot;
    const t = tabs[props.source];
    if (s?.heap) return t === "biggest" ? "biggest" : "summary";
    return t && t !== "summary" && t !== "biggest" ? t : "stats";
  };
  return (
    <div class="prof-view profiling-view" data-testid={`profiler-${props.source}`}>
      <Show
        when={l()}
        fallback={
          <div class="prof-empty muted">
            <p>No {SOURCE_LABELS[props.source]} snapshot yet.</p>
            <p>{EMPTY[props.source]}</p>
          </div>
        }
      >
        {(loaded) => {
          const s = () => loaded().snapshot;
          return (
            <>
              <div class="prof-toolbar">
                <span class="prof-title" title={[loaded().path, ...s().info.map((i) => `${i.label}: ${i.value}`)].join("\n")}>
                  {s().title}
                </span>
                <Show when={s().metrics.length > 1}>
                  <select
                    class="prof-type"
                    value={s().metric}
                    disabled={profilerBusy() === props.source}
                    onChange={(e) => void switchMetric(props.source, Number(e.currentTarget.value))}
                  >
                    <For each={s().metrics}>{(m, i) => <option value={i()}>{m.kind}</option>}</For>
                  </select>
                </Show>
                <span class="muted">{summary(s())}</span>
                <span class="prof-spacer" />
                <div class="prof-tabs">
                  <Show
                    when={!s().heap}
                    fallback={
                      <>
                        <TabButton source={props.source} tab="summary" current={tab()} label="Summary" icon={HardDrive} />
                        <TabButton source={props.source} tab="biggest" current={tab()} label="Biggest Objects" icon={Rows3} />
                      </>
                    }
                  >
                    <TabButton source={props.source} tab="stats" current={tab()} label="Execution Statistics" icon={Table2} />
                    <TabButton source={props.source} tab="tree" current={tab()} label="Call Tree" icon={ListTree} />
                    <TabButton source={props.source} tab="flame" current={tab()} label="Flame Graph" icon={Flame} />
                  </Show>
                </div>
              </div>
              <Show when={s().heap}>{(h) => <MemoryView heap={h()} tab={tab()} />}</Show>
              <Show when={!s().heap && tab() === "stats"}>
                <ExecutionStatistics snapshot={s()} />
              </Show>
              <Show when={!s().heap && tab() !== "stats" && s().view}>
                {(v) => <ProfileBody view={v()} tab={tab() === "flame" ? "flame" : "tree"} style={style} />}
              </Show>
            </>
          );
        }}
      </Show>
    </div>
  );
}
