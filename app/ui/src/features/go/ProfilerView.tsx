/**
 * The Profiler tool window, laid out like GoLand's: the profile's name and sample type on
 * top, then Flame Graph, Call Tree and Top (method list) tabs. A pprof profile (a profiled
 * Go Test run, or a `.pprof` file opened with Open Profile) is decoded in Rust
 * (`pprof_open`, `goforge-pprof`); this view only draws it. Clicking a frame opens its
 * source when it is a project file.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, Flame, FolderOpen, ListTree, Rows3 } from "lucide-solid";
import { goApi, type CallNode, type ProfileView as Profile } from "@/ipc/go";
import { openFileAt } from "@/editor/views";
import { setInputDialog } from "@/features/git/overlays";
import { showToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { showGoBalloon } from "./state";
import "./profiler.css";

interface Opened {
  path: string;
  title: string;
  view: Profile;
}

const [opened, setOpened] = createSignal<Opened | null>(null);
const [tab, setTab] = createSignal<"flame" | "tree" | "top">("flame");

/** Open a profile in the Profiler (an absolute path or a project file). */
export async function openProfile(path: string, title?: string, sampleIndex?: number): Promise<void> {
  try {
    const view = await goApi.pprofOpen(path, sampleIndex);
    setOpened({ path, title: title ?? path.replace(/\\/g, "/").split("/").pop() ?? path, view });
    showToolWindow("profiler");
  } catch (e) {
    showGoBalloon("Cannot open the profile", String(e).replace(/^Error: /, ""), 8000);
  }
}

export function askForProfile(): void {
  setInputDialog({
    title: "Open Profile",
    label: "pprof file (absolute path or project file):",
    value: "cpu.pprof",
    okLabel: "Open",
    onOk: (value) => openProfile(value.trim()),
  });
}

/** The sample type's unit as text. */
export function formatValue(v: number, unit: string): string {
  const abs = Math.abs(v);
  if (unit === "nanoseconds") {
    if (abs >= 1e9) return `${(v / 1e9).toFixed(2)} s`;
    if (abs >= 1e6) return `${(v / 1e6).toFixed(abs >= 1e8 ? 0 : 1)} ms`;
    if (abs >= 1e3) return `${(v / 1e3).toFixed(1)} µs`;
    return `${v} ns`;
  }
  if (unit === "bytes") {
    if (abs >= 1 << 30) return `${(v / (1 << 30)).toFixed(2)} GB`;
    if (abs >= 1 << 20) return `${(v / (1 << 20)).toFixed(1)} MB`;
    if (abs >= 1 << 10) return `${(v / (1 << 10)).toFixed(1)} KB`;
    return `${v} B`;
  }
  return v.toLocaleString("en-US");
}

const percent = (v: number, total: number) => (total ? `${((v / total) * 100).toFixed(1)}%` : "0%");

/** `github.com/acme/ledger/internal/money.Amount.String` → `money.Amount.String`. */
export const shortName = (name: string) => name.replace(/^.*\//, "");

const isProjectFile = (file: string) => {
  const root = workspace()?.root ?? "";
  return !!file && (!file.startsWith("/") || (!!root && file.startsWith(`${root}/`))) && !file.includes("/go/src/");
};

function navigate(file: string, line: number) {
  if (!isProjectFile(file)) return;
  const root = workspace()?.root ?? "";
  void openFileAt(file.startsWith(`${root}/`) ? file.slice(root.length + 1) : file, line || 1);
}

/** A stable color per package, from the theme's graph colors. */
function colorOf(name: string): string {
  if (name === "root") return "var(--bg-active)";
  if (/^(runtime|internal\/|syscall)/.test(shortName(name)) || name.startsWith("runtime.")) return "var(--graph-5)";
  if (/^(fmt|strconv|strings|bytes|sort|sync|reflect|os|io|testing|encoding)\b/.test(name)) return "var(--graph-3)";
  let h = 0;
  const pkg = name.replace(/\.[^./]*(\.[^./]*)?$/, "");
  for (const c of pkg) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return `var(--graph-${[0, 1, 2, 4, 6][h % 5]})`;
}

function FlameGraph(props: { root: CallNode; unit: string }) {
  const [focus, setFocus] = createSignal<CallNode | null>(null);
  const base = () => focus() ?? props.root;
  /** Rows of frames (root first) with their offset and width as fractions of the base. */
  const rows = createMemo(() => {
    const out: { node: CallNode; x: number; w: number }[][] = [];
    const total = base().total || 1;
    const walk = (n: CallNode, depth: number, x: number) => {
      const w = n.total / total;
      if (w < 0.002) return;
      (out[depth] ??= []).push({ node: n, x, w });
      let cx = x;
      for (const c of n.children) {
        walk(c, depth + 1, cx);
        cx += c.total / total;
      }
    };
    walk(base(), 0, 0);
    return out;
  });
  return (
    <div class="prof-flame" data-testid="profiler-flame">
      <For each={[...rows()].reverse()}>
        {(row) => (
          <div class="prof-flame-row">
            <For each={row}>
              {(f) => (
                <div
                  class="prof-flame-frame"
                  style={{ left: `${f.x * 100}%`, width: `${f.w * 100}%`, background: colorOf(f.node.name) }}
                  title={`${f.node.name}\n${formatValue(f.node.total, props.unit)} (${percent(f.node.total, props.root.total)}), self ${formatValue(f.node.self, props.unit)}${f.node.file ? `\n${f.node.file}:${f.node.line}` : ""}`}
                  onClick={() => setFocus(f.node === base() ? null : f.node)}
                  onDblClick={() => navigate(f.node.file, f.node.line)}
                >
                  <span>{f.node.name === "root" ? "all" : shortName(f.node.name)}</span>
                </div>
              )}
            </For>
          </div>
        )}
      </For>
      <Show when={focus()}>
        <button class="prof-reset pill-btn" onClick={() => setFocus(null)}>
          Reset zoom
        </button>
      </Show>
    </div>
  );
}

function TreeRow(props: { node: CallNode; depth: number; total: number; unit: string }) {
  const [open, setOpen] = createSignal(props.depth < 6 && props.node.total / (props.total || 1) > 0.05);
  const has = () => props.node.children.length > 0;
  return (
    <>
      <div class="tree-row prof-tree-row" style={{ "padding-left": `${8 + props.depth * 14}px` }} onDblClick={() => navigate(props.node.file, props.node.line)}>
        <span class="prof-toggle" onClick={() => setOpen(!open())}>
          <Show when={has()}>{open() ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
        </span>
        <span class="prof-pct">{percent(props.node.total, props.total)}</span>
        <span class="prof-bar">
          <span style={{ width: percent(props.node.total, props.total) }} />
        </span>
        <span class="prof-name">{props.node.name === "root" ? "All" : shortName(props.node.name)}</span>
        <span class="muted prof-value">{formatValue(props.node.total, props.unit)}</span>
        <Show when={props.node.self}>
          <span class="muted prof-self">self {formatValue(props.node.self, props.unit)}</span>
        </Show>
      </div>
      <Show when={open()}>
        <For each={props.node.children}>{(c) => <TreeRow node={c} depth={props.depth + 1} total={props.total} unit={props.unit} />}</For>
      </Show>
    </>
  );
}

export function ProfilerView() {
  const unit = () => {
    const v = opened()?.view;
    return v ? (v.sampleTypes[v.sampleIndex]?.unit ?? "") : "";
  };
  return (
    <div class="prof-view">
      <Show
        when={opened()}
        fallback={
          <div class="prof-empty muted">
            <p>No profile is open.</p>
            <p>Run a Go test or benchmark with a profile (Go Test configuration › Profile), or open a pprof file.</p>
            <button class="btn-secondary" onClick={askForProfile}>
              Open Profile…
            </button>
          </div>
        }
      >
        {(o) => (
          <>
            <div class="prof-toolbar">
              <span class="prof-title">{o().title}</span>
              <select
                class="prof-type"
                value={String(o().view.sampleIndex)}
                onChange={(e) => void openProfile(o().path, o().title, Number(e.currentTarget.value))}
              >
                <For each={o().view.sampleTypes}>
                  {(t, i) => (
                    <option value={String(i())}>
                      {t.kind} ({t.unit})
                    </option>
                  )}
                </For>
              </select>
              <span class="muted">
                Total {formatValue(o().view.total, unit())} · {o().view.samples} samples
                <Show when={o().view.durationNanos}> · {formatValue(o().view.durationNanos, "nanoseconds")} recorded</Show>
              </span>
              <span class="prof-spacer" />
              <div class="prof-tabs">
                <button classList={{ active: tab() === "flame" }} onClick={() => setTab("flame")}>
                  <Flame size={13} /> Flame Graph
                </button>
                <button classList={{ active: tab() === "tree" }} onClick={() => setTab("tree")}>
                  <ListTree size={13} /> Call Tree
                </button>
                <button classList={{ active: tab() === "top" }} onClick={() => setTab("top")}>
                  <Rows3 size={13} /> Method List
                </button>
              </div>
              <button class="icon-btn" title="Open Profile…" onClick={askForProfile}>
                <FolderOpen size={14} />
              </button>
            </div>
            <div class="prof-body">
              <Show when={tab() === "flame"}>
                <FlameGraph root={o().view.tree} unit={unit()} />
              </Show>
              <Show when={tab() === "tree"}>
                <div class="prof-tree">
                  <TreeRow node={o().view.tree} depth={0} total={o().view.total} unit={unit()} />
                </div>
              </Show>
              <Show when={tab() === "top"}>
                <table class="prof-top">
                  <thead>
                    <tr>
                      <th>Method</th>
                      <th>Self</th>
                      <th>Self %</th>
                      <th>Total</th>
                      <th>Total %</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={o().view.top}>
                      {(r) => (
                        <tr onDblClick={() => navigate(r.file, r.line)} classList={{ "prof-own": isProjectFile(r.file) }}>
                          <td title={`${r.name}\n${r.file}`}>{shortName(r.name)}</td>
                          <td>{formatValue(r.flat, unit())}</td>
                          <td>{percent(r.flat, o().view.total)}</td>
                          <td>{formatValue(r.cum, unit())}</td>
                          <td>{percent(r.cum, o().view.total)}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </Show>
            </div>
          </>
        )}
      </Show>
    </div>
  );
}
