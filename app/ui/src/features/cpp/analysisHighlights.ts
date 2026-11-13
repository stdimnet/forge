/**
 * Editor highlights of a sanitizer report or Valgrind error: the line where it happened
 * (red tint) and the other project frames of its stacks (yellow tint), with the message on
 * hover, like CLion's highlighting of the selected sanitizer problem. Set from the result
 * panels when an issue is selected; "Clear Highlights" removes them.
 */
import { createSignal } from "solid-js";
import { Decoration, EditorView, ViewPlugin, type DecorationSet } from "@codemirror/view";
import { RangeSetBuilder, StateEffect, StateField, type Extension } from "@codemirror/state";
import type { Stack, StackFrame } from "@/ipc/cppTools";

export interface LineHighlight {
  path: string;
  line: number;
  /** `issue`: where it happened; `frame`: another frame of its stacks. */
  kind: "issue" | "frame";
  message: string;
}

const [highlights, setHighlightsSignal] = createSignal<LineHighlight[]>([]);
export { highlights as analysisHighlights };

const views = new Map<EditorView, string>();
const replace = StateEffect.define<DecorationSet>();

const shown = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(replace)) return e.value;
    return value.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

function decorations(view: EditorView, list: LineHighlight[]): DecorationSet {
  const doc = view.state.doc;
  const byLine = new Map<number, LineHighlight>();
  for (const h of list) {
    if (h.line < 1 || h.line > doc.lines) continue;
    const prev = byLine.get(h.line);
    if (!prev || (prev.kind === "frame" && h.kind === "issue")) byLine.set(h.line, h);
  }
  const b = new RangeSetBuilder<Decoration>();
  for (const n of [...byLine.keys()].sort((a, c) => a - c)) {
    const h = byLine.get(n)!;
    const from = doc.line(n).from;
    b.add(from, from, Decoration.line({ class: `cm-native-${h.kind}`, attributes: { title: h.message } }));
  }
  return b.finish();
}

function push(view: EditorView, path: string) {
  view.dispatch({ effects: replace.of(decorations(view, highlights().filter((h) => h.path === path))) });
}

/** Highlight these lines in open and newly opened editors. */
export function setAnalysisHighlights(list: LineHighlight[]): void {
  setHighlightsSignal(list);
  for (const [view, path] of views) push(view, path);
}

/** The highlights of an issue: its location and the project frames of its stacks. */
export function highlightsOf(message: string, location: StackFrame | undefined, stacks: Stack[]): LineHighlight[] {
  const out: LineHighlight[] = [];
  if (location?.path && location.line) out.push({ path: location.path, line: location.line, kind: "issue", message });
  for (const s of stacks)
    for (const f of s.frames)
      if (f.path && f.line && !(f.path === location?.path && f.line === location?.line))
        out.push({ path: f.path, line: f.line, kind: "frame", message: `${s.title}: ${f.function}` });
  return out;
}

/** The editor extension (every file; only files with highlights get decorations). */
export function analysisHighlightExtension(path: string): Extension {
  return [
    shown,
    ViewPlugin.define((view) => {
      views.set(view, path);
      if (highlights().some((h) => h.path === path)) queueMicrotask(() => views.has(view) && push(view, path));
      return { destroy: () => views.delete(view) };
    }),
  ];
}
