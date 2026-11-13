/**
 * The views of a profile shared by the Profiler's sources (GoForge's pprof, CForge's perf),
 * laid out like GoLand's and CLion's profilers: a toolbar with the profile's name and
 * Flame Graph, Call Tree and Method List tabs. The data comes computed from Rust
 * (`goforge_pprof::ProfileView`); this module only draws it. Sources pass how frames map
 * to project files (`navigate`, `isOwn`), names are shortened (`shortName`) and colored
 * (`colorOf`).
 */
import { createMemo, createSignal, For, Show, type JSX } from "solid-js";
import { ChevronDown, ChevronRight, Flame, ListTree, Rows3 } from "lucide-solid";
import type { CallNode, ProfileView } from "@/ipc/profile";
import { t as tr } from "@/i18n";
import "./callTree.css";

export type ProfileTab = "flame" | "tree" | "top";

/** How a source shows its frames. */
export interface FrameStyle {
  /** Open a frame's source (only called for project files). */
  navigate: (file: string, line: number) => void;
  /** Whether a file belongs to the project (highlighted, navigable). */
  isOwn: (file: string) => boolean;
  shortName?: (name: string) => string;
  colorOf?: (name: string) => string;
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

export const percent = (v: number, total: number) => (total ? `${((v / total) * 100).toFixed(1)}%` : "0%");

/** A stable theme graph color per name prefix. */
export function hashColor(key: string): string {
  let h = 0;
  for (const c of key) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return `var(--graph-${[0, 1, 2, 4, 6][h % 5]})`;
}

const short = (style: FrameStyle, name: string) => (name === "root" ? "all" : (style.shortName?.(name) ?? name));

function FlameGraph(props: { root: CallNode; unit: string; style: FrameStyle }) {
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
  const color = (name: string) => (name === "root" ? "var(--bg-active)" : (props.style.colorOf?.(name) ?? hashColor(name)));
  return (
    <div class="prof-flame" data-testid="profiler-flame">
      <For each={[...rows()].reverse()}>
        {(row) => (
          <div class="prof-flame-row">
            <For each={row}>
              {(f) => (
                <div
                  class="prof-flame-frame"
                  style={{ left: `${f.x * 100}%`, width: `${f.w * 100}%`, background: color(f.node.name), color: f.node.name === "root" ? "var(--fg)" : undefined }}
                  title={tr("tools.profiler.self", { name: f.node.name, value: formatValue(f.node.total, props.unit), value2: percent(f.node.total, props.root.total), value3: formatValue(f.node.self, props.unit), value4: f.node.file ? `\n${f.node.file}:${f.node.line}` : "" })}
                  onClick={() => setFocus(f.node === base() ? null : f.node)}
                  onDblClick={() => props.style.isOwn(f.node.file) && props.style.navigate(f.node.file, f.node.line)}
                >
                  <span>{short(props.style, f.node.name)}</span>
                </div>
              )}
            </For>
          </div>
        )}
      </For>
      <Show when={focus()}>
        <button class="prof-reset pill-btn" onClick={() => setFocus(null)}>
          {tr("tools.profiler.resetZoom")}
        </button>
      </Show>
    </div>
  );
}

function TreeRow(props: { node: CallNode; depth: number; total: number; unit: string; style: FrameStyle }) {
  const [open, setOpen] = createSignal(props.depth < 6 && props.node.total / (props.total || 1) > 0.05);
  const has = () => props.node.children.length > 0;
  return (
    <>
      <div
        class="tree-row prof-tree-row"
        style={{ "padding-left": `${8 + props.depth * 14}px` }}
        onDblClick={() => props.style.isOwn(props.node.file) && props.style.navigate(props.node.file, props.node.line)}
      >
        <span class="prof-toggle" onClick={() => setOpen(!open())}>
          <Show when={has()}>{open() ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
        </span>
        <span class="prof-pct">{percent(props.node.total, props.total)}</span>
        <span class="prof-bar">
          <span style={{ width: percent(props.node.total, props.total) }} />
        </span>
        <span class="prof-name" classList={{ "prof-own": props.style.isOwn(props.node.file) }}>
          {props.node.name === "root" ? tr("common.all") : short(props.style, props.node.name)}
        </span>
        <span class="muted prof-value">{formatValue(props.node.total, props.unit)}</span>
        <Show when={props.node.self}>
          <span class="muted prof-self">self {formatValue(props.node.self, props.unit)}</span>
        </Show>
      </div>
      <Show when={open()}>
        <For each={props.node.children}>{(c) => <TreeRow node={c} depth={props.depth + 1} total={props.total} unit={props.unit} style={props.style} />}</For>
      </Show>
    </>
  );
}

/** Flame Graph, Call Tree and Method List tab buttons. */
export function ProfileTabs(props: { tab: ProfileTab; onTab: (t: ProfileTab) => void }) {
  return (
    <div class="prof-tabs">
      <button classList={{ active: props.tab === "flame" }} onClick={() => props.onTab("flame")}>
        <Flame size={13} /> {tr("tools.profiler.flameGraph")}
      </button>
      <button classList={{ active: props.tab === "tree" }} onClick={() => props.onTab("tree")}>
        <ListTree size={13} /> {tr("common.callTree")}
      </button>
      <button classList={{ active: props.tab === "top" }} onClick={() => props.onTab("top")}>
        <Rows3 size={13} /> {tr("tools.profiler.methodList")}
      </button>
    </div>
  );
}

/** The unit of the profile's shown sample type. */
export const unitOf = (v: ProfileView) => v.sampleTypes[v.sampleIndex]?.unit ?? "";

/** The body of a profile: the chosen tab's view. */
export function ProfileBody(props: { view: ProfileView; tab: ProfileTab; style: FrameStyle }) {
  const unit = () => unitOf(props.view);
  return (
    <div class="prof-body">
      <Show when={props.tab === "flame"}>
        <FlameGraph root={props.view.tree} unit={unit()} style={props.style} />
      </Show>
      <Show when={props.tab === "tree"}>
        <div class="prof-tree">
          <TreeRow node={props.view.tree} depth={0} total={props.view.total} unit={unit()} style={props.style} />
        </div>
      </Show>
      <Show when={props.tab === "top"}>
        <table class="prof-top">
          <thead>
            <tr>
              <th>{tr("common.method")}</th>
              <th>{tr("tools.profiler.self2")}</th>
              <th>{tr("tools.profiler.self3")}</th>
              <th>{tr("tools.profiler.total")}</th>
              <th>{tr("tools.profiler.total2")}</th>
            </tr>
          </thead>
          <tbody>
            <For each={props.view.top}>
              {(r) => (
                <tr onDblClick={() => props.style.isOwn(r.file) && props.style.navigate(r.file, r.line)} classList={{ "prof-own": props.style.isOwn(r.file) }}>
                  <td title={`${r.name}\n${r.file}`}>{props.style.shortName?.(r.name) ?? r.name}</td>
                  <td>{formatValue(r.flat, unit())}</td>
                  <td>{percent(r.flat, props.view.total)}</td>
                  <td>{formatValue(r.cum, unit())}</td>
                  <td>{percent(r.cum, props.view.total)}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </div>
  );
}

/** The toolbar of a profile view: title, extra controls, totals and the tabs. */
export function ProfileToolbar(props: { title: string; children?: JSX.Element; summary: string; tab: ProfileTab; onTab: (t: ProfileTab) => void }) {
  return (
    <div class="prof-toolbar">
      <span class="prof-title">{props.title}</span>
      {props.children}
      <span class="muted">{props.summary}</span>
      <span class="prof-spacer" />
      <ProfileTabs tab={props.tab} onTab={props.onTab} />
    </div>
  );
}
