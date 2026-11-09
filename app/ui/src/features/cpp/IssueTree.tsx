/**
 * The tree of CForge's dynamic analysis results (Sanitizers and Valgrind tabs of the Run
 * tool window), laid out like CLion's: issues (optionally in groups) with their stacks and
 * frames on the left, the selected issue's details and a preview of the selected frame's
 * code on the right. Clicking an issue or frame highlights its lines in the editor;
 * double-clicking a project frame (or Enter) opens it.
 */
import { createEffect, createMemo, createResource, createSignal, For, Show, type Component, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronDown, ChevronRight, Eraser, Layers } from "lucide-solid";
import { openFileAt } from "@/editor/views";
import { workspaceApi } from "@/ipc/workspace";
import type { Stack, StackFrame } from "@/ipc/cppTools";
import { highlightsOf, setAnalysisHighlights } from "./analysisHighlights";
import { t as tr } from "@/i18n";

export interface Issue {
  id: string;
  /** Group header the issue is listed under ("Errors", "Leaks"). */
  group?: string;
  icon: Component<{ size?: number }>;
  /** CSS color of the icon. */
  color: string;
  /** `heap-buffer-overflow`, `Invalid read of size 4`. */
  title: string;
  message: string;
  details?: string[];
  summary?: string;
  /** "×3", "48 bytes". */
  badge?: string;
  location?: StackFrame;
  stacks: Stack[];
}

const baseName = (p: string) => p.replace(/\\/g, "/").split("/").pop() ?? p;

export const frameLocation = (f: StackFrame) =>
  f.file ? `${baseName(f.path ?? f.file)}:${f.line}${f.column ? `:${f.column}` : ""}` : (f.module ?? "");

/** Open a frame's location; `openFileAt` takes the 1-based column as reported (exported for tests). */
export function openFrame(f: StackFrame | undefined) {
  if (f?.path) void openFileAt(f.path, f.line || 1, f.column || undefined, { flash: true });
}

/** A few lines around a frame's line, with that line marked. */
function Preview(props: { frame: StackFrame }) {
  const [text] = createResource(
    () => props.frame.path,
    (p) => workspaceApi.readFile(p).catch(() => null),
  );
  const lines = createMemo(() => {
    const t = text();
    if (!t) return [];
    const all = t.split("\n");
    const at = props.frame.line;
    const from = Math.max(1, at - 4);
    const to = Math.min(all.length, at + 4);
    const out: { n: number; text: string }[] = [];
    for (let n = from; n <= to; n++) out.push({ n, text: all[n - 1] });
    return out;
  });
  return (
    <Show when={lines().length}>
      <div class="native-preview" onDblClick={() => openFrame(props.frame)}>
        <div class="native-preview-title muted">
          {props.frame.path}:{props.frame.line}
        </div>
        <pre>
          <For each={lines()}>
            {(l) => (
              <div class="native-preview-line" classList={{ current: l.n === props.frame.line }}>
                <span class="native-preview-n">{l.n}</span>
                {l.text || " "}
              </div>
            )}
          </For>
        </pre>
      </div>
    </Show>
  );
}

function FrameRow(props: { frame: StackFrame; selected: boolean; onSelect: () => void }) {
  return (
    <div
      class="tree-row native-row native-frame"
      classList={{ selected: props.selected, "native-own": !!props.frame.path }}
      style={{ "padding-left": "52px" }}
      data-native-frame={props.frame.index}
      onClick={props.onSelect}
      onDblClick={() => openFrame(props.frame)}
    >
      <span class="native-index muted">#{props.frame.index}</span>
      <span class="native-function">{props.frame.function}</span>
      <span class="native-location" classList={{ muted: !props.frame.path }}>
        {frameLocation(props.frame)}
      </span>
    </div>
  );
}

export function IssueTree(props: { issues: Issue[]; header?: JSX.Element; empty: string }) {
  const [selected, setSelected] = createSignal<string | null>(null);
  const [frame, setFrame] = createSignal<StackFrame | null>(null);
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
  const toggle = (set: () => Set<string>, update: (s: Set<string>) => void, key: string) => {
    const next = new Set(set());
    if (next.has(key)) next.delete(key);
    else next.add(key);
    update(next);
  };
  const current = () => props.issues.find((i) => i.id === selected()) ?? props.issues[0];
  const groups = createMemo(() => {
    const out: { name?: string; issues: Issue[] }[] = [];
    for (const i of props.issues) {
      const g = out.find((x) => x.name === i.group);
      if (g) g.issues.push(i);
      else out.push({ name: i.group, issues: [i] });
    }
    return out;
  });
  const choose = (i: Issue, f: StackFrame | null) => {
    setSelected(i.id);
    setFrame(f);
    setAnalysisHighlights(highlightsOf(`${i.title}: ${i.message}`, i.location, i.stacks));
  };
  // The first issue is selected and expanded when results arrive.
  createEffect(() => {
    const first = props.issues[0];
    if (first && !props.issues.some((i) => i.id === selected())) {
      setSelected(first.id);
      setExpanded(new Set([first.id]));
      setFrame(first.location ?? null);
    }
  });
  const previewFrame = () => frame() ?? current()?.location ?? null;
  return (
    <div class="native-results">
      <Show when={props.header}>
        <div class="native-header">
          {props.header}
          <span class="native-spacer" />
          <button class="icon-btn" title={tr("langs.cpp.clearHighlights")} onClick={() => setAnalysisHighlights([])}>
            <Eraser size={14} />
          </button>
        </div>
      </Show>
      <Show when={props.issues.length} fallback={<div class="native-empty muted">{props.empty}</div>}>
        <div class="native-split">
          <div class="native-tree" role="tree">
            <For each={groups()}>
              {(g) => (
                <>
                  <Show when={g.name}>
                    <div class="tree-row native-row native-group" onClick={() => toggle(collapsed, setCollapsed, g.name!)}>
                      {collapsed().has(g.name!) ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
                      <span>{g.name}</span>
                      <span class="muted">{g.issues.length}</span>
                    </div>
                  </Show>
                  <Show when={!g.name || !collapsed().has(g.name)}>
                    <For each={g.issues}>
                      {(i) => (
                        <>
                          <div
                            class="tree-row native-row native-issue"
                            classList={{ selected: current()?.id === i.id && !frame() }}
                            style={{ "padding-left": g.name ? "20px" : "6px" }}
                            data-native-issue={i.id}
                            onClick={() => {
                              toggle(expanded, setExpanded, i.id);
                              choose(i, null);
                            }}
                            onDblClick={() => openFrame(i.location)}
                          >
                            {expanded().has(i.id) ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                            <span class="native-icon" style={{ color: i.color }}>
                              <Dynamic component={i.icon} size={14} />
                            </span>
                            <span class="native-title">{i.title}</span>
                            <Show when={i.location}>{(l) => <span class="native-location">{frameLocation(l())}</span>}</Show>
                            <Show when={i.badge}>
                              <span class="native-badge">{i.badge}</span>
                            </Show>
                          </div>
                          <Show when={expanded().has(i.id)}>
                            <For each={i.stacks}>
                              {(s, si) => {
                                const key = `${i.id}/${si()}`;
                                return (
                                  <>
                                    <div
                                      class="tree-row native-row native-stack"
                                      style={{ "padding-left": g.name ? "36px" : "22px" }}
                                      onClick={() => toggle(collapsed, setCollapsed, key)}
                                    >
                                      {collapsed().has(key) ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
                                      <Layers size={13} class="muted" />
                                      <span>{s.title}</span>
                                    </div>
                                    <Show when={!collapsed().has(key)}>
                                      <For each={s.frames}>
                                        {(f) => <FrameRow frame={f} selected={frame() === f} onSelect={() => choose(i, f)} />}
                                      </For>
                                    </Show>
                                  </>
                                );
                              }}
                            </For>
                          </Show>
                        </>
                      )}
                    </For>
                  </Show>
                </>
              )}
            </For>
          </div>
          <div class="native-details">
            <Show when={current()}>
              {(i) => (
                <>
                  <div class="native-message">{i().message}</div>
                  <For each={i().details ?? []}>{(d) => <div class="native-detail muted">{d}</div>}</For>
                  <Show when={i().summary}>
                    <div class="native-summary muted">{tr("langs.cpp.summary", { summary: i().summary })}</div>
                  </Show>
                  <Show when={previewFrame()?.path ? previewFrame() : null} keyed>
                    {(f) => <Preview frame={f} />}
                  </Show>
                </>
              )}
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
}
