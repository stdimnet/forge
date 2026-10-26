/**
 * Coverage marks in the editor gutter, like PHPStorm: a green bar on covered lines, yellow on
 * partly covered ones (some branches not taken) and red on lines that never ran; hovering a
 * mark shows the hits ("Hits: 3, branches: 1 of 2 covered"). The marks come from Rust
 * (`coverage_file`).
 */
import { gutter, GutterMarker, ViewPlugin, type EditorView } from "@codemirror/view";
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import type { LineMark } from "@/ipc/testing";
import { marksFor, onCoverageChange } from "./store";

const setMarks = StateEffect.define<LineMark[]>();

const marksField = StateField.define<Map<number, LineMark>>({
  create: () => new Map(),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setMarks)) return new Map(e.value.map((m) => [m.line, m]));
    return value;
  },
});

class CoverageMarker extends GutterMarker {
  constructor(readonly mark: LineMark) {
    super();
  }
  eq(other: CoverageMarker) {
    return other.mark.status === this.mark.status && other.mark.hint === this.mark.hint;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = `coverage-mark ${this.mark.status}`;
    el.title = this.mark.hint;
    el.dataset.coverage = this.mark.status;
    return el;
  }
}

const views = new Map<EditorView, string>();

async function load(view: EditorView, path: string) {
  const list = await marksFor(path);
  if (!views.has(view)) return;
  view.dispatch({ effects: setMarks.of(list) });
}

onCoverageChange(() => {
  for (const [v, path] of views) void load(v, path);
});

/** The coverage gutter of a project file. */
export function coverageGutter(path: string): Extension {
  return [
    marksField,
    ViewPlugin.define((view) => {
      views.set(view, path);
      void load(view, path);
      return { destroy: () => views.delete(view) };
    }),
    gutter({
      class: "cm-coverage-gutter",
      lineMarker(view, line) {
        const n = view.state.doc.lineAt(line.from).number;
        const m = view.state.field(marksField).get(n);
        return m ? new CoverageMarker(m) : null;
      },
      lineMarkerChange: (u) => u.transactions.some((tr) => tr.effects.some((e) => e.is(setMarks))),
    }),
  ];
}
