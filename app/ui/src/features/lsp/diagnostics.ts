/**
 * Diagnostics in the editor: squiggles per severity (error / warning / weak warning),
 * greyed unused code, struck-through deprecated usages, gutter markers, the PHPStorm
 * inspection widget in the top-right corner and F2 / Shift+F2 navigation.
 */
import { RangeSet, RangeSetBuilder, StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  GutterMarker,
  ViewPlugin,
  gutter,
  keymap,
  type DecorationSet,
  type PluginValue,
  type ViewUpdate,
} from "@codemirror/view";
import { Check, ChevronDown, ChevronUp } from "lucide-solid";
import type { EditorDiagnostic, Severity } from "@/ipc/lsp";
import { showToolWindow } from "@/state/layout";
import { countBySeverity, diagnostics as diagnosticsStore, onDiagnostics, severityRank } from "./store";
import { pluralize, severityIcon, severityLabel } from "./severity";
import { fromLsp, h, iconNode } from "./util";

/** A diagnostic resolved to document offsets. */
export interface PlacedDiagnostic {
  from: number;
  to: number;
  diag: EditorDiagnostic;
}

interface DiagState {
  items: PlacedDiagnostic[];
  decorations: DecorationSet;
}

export const setDiagnosticsEffect = StateEffect.define<EditorDiagnostic[]>();

function place(state: EditorState, diags: EditorDiagnostic[]): PlacedDiagnostic[] {
  const doc = state.doc;
  return diags
    .filter((diag) => !diag.fileLevel)
    .map((diag) => {
      let from = fromLsp(doc, diag.range.start);
      let to = fromLsp(doc, diag.range.end);
      if (to <= from) {
        // Zero-width ranges get the word (or character) at that position.
        const word = state.wordAt(from);
        if (word) ({ from, to } = word);
        else if (from < doc.lineAt(from).to) to = from + 1;
        else if (from > doc.lineAt(from).from) from -= 1;
      }
      return { from, to, diag };
    })
    .sort((a, b) => a.from - b.from || severityRank[a.diag.severity] - severityRank[b.diag.severity]);
}

function decorate(items: PlacedDiagnostic[]): DecorationSet {
  const ranges = items
    .filter((i) => i.to > i.from)
    .map(({ from, to, diag }) => {
      const cls = [`cm-lsp-diag cm-lsp-diag-${diag.severity}`];
      if (diag.unnecessary) cls.push("cm-lsp-unused");
      if (diag.deprecated) cls.push("cm-lsp-deprecated");
      return Decoration.mark({ class: cls.join(" ") }).range(from, to);
    });
  return Decoration.set(ranges, true);
}

export const diagnosticsField = StateField.define<DiagState>({
  create: () => ({ items: [], decorations: Decoration.none }),
  update(value, tr) {
    for (const e of tr.effects) {
      if (e.is(setDiagnosticsEffect)) {
        const items = place(tr.state, e.value);
        return { items, decorations: decorate(items) };
      }
    }
    if (!tr.docChanged) return value;
    const items = value.items
      .map((i) => ({ ...i, from: tr.changes.mapPos(i.from, 1), to: tr.changes.mapPos(i.to, -1) }))
      .filter((i) => i.to >= i.from);
    return { items, decorations: value.decorations.map(tr.changes) };
  },
  provide: (f) => EditorView.decorations.from(f, (v) => v.decorations),
});

/** Diagnostics whose range touches `pos`, most severe first. */
export function diagnosticsAt(state: EditorState, pos: number): EditorDiagnostic[] {
  return state
    .field(diagnosticsField, false)
    ?.items.filter((i) => i.from <= pos && i.to >= pos)
    .map((i) => i.diag)
    .sort((a, b) => severityRank[a.severity] - severityRank[b.severity]) ?? [];
}

class SeverityMarker extends GutterMarker {
  constructor(
    readonly severity: Severity,
    readonly message: string,
  ) {
    super();
  }
  eq(other: SeverityMarker) {
    return other.severity === this.severity && other.message === this.message;
  }
  toDOM() {
    const el = h("div", `cm-lsp-gutter-marker sev-${this.severity}`, iconNode(severityIcon[this.severity], 12));
    el.title = this.message;
    return el;
  }
}

function gutterMarkers(state: EditorState): RangeSet<GutterMarker> {
  const items = state.field(diagnosticsField, false)?.items ?? [];
  const byLine = new Map<number, PlacedDiagnostic[]>();
  for (const item of items) {
    if (item.diag.severity === "info" || item.diag.severity === "typo" || item.diag.severity === "grammar") continue;
    const line = state.doc.lineAt(item.from).from;
    byLine.set(line, [...(byLine.get(line) ?? []), item]);
  }
  const builder = new RangeSetBuilder<GutterMarker>();
  for (const line of [...byLine.keys()].sort((a, b) => a - b)) {
    const list = byLine.get(line)!.sort((a, b) => severityRank[a.diag.severity] - severityRank[b.diag.severity]);
    builder.add(line, line, new SeverityMarker(list[0].diag.severity, list.map((i) => i.diag.message).join("\n")));
  }
  return builder.finish();
}

const diagnosticsGutter = gutter({
  class: "cm-lsp-gutter",
  markers: (view) => gutterMarkers(view.state),
  initialSpacer: () => new SeverityMarker("weak", ""),
});

/** Move the cursor to the next/previous problem (PHPStorm: F2 / Shift+F2). */
export function gotoProblem(view: EditorView, dir: 1 | -1): boolean {
  const items = view.state.field(diagnosticsField, false)?.items ?? [];
  if (!items.length) return false;
  const head = view.state.selection.main.head;
  const target =
    dir > 0
      ? (items.find((i) => i.from > head) ?? items[0])
      : ([...items].reverse().find((i) => i.from < head) ?? items[items.length - 1]);
  view.dispatch({ selection: { anchor: target.from }, effects: EditorView.scrollIntoView(target.from, { y: "center" }) });
  view.focus();
  return true;
}

/** The ✓ / error & warning counter in the editor's top-right corner. */
class InspectionWidget implements PluginValue {
  dom: HTMLElement;
  private signature = "";

  constructor(readonly view: EditorView) {
    this.dom = h("div", "cm-lsp-inspection");
    this.dom.setAttribute("data-testid", "inspection-widget");
    view.dom.appendChild(this.dom);
    this.render();
  }

  update(u: ViewUpdate) {
    if (u.startState.field(diagnosticsField, false) !== u.state.field(diagnosticsField, false)) this.render();
  }

  render() {
    const items = this.view.state.field(diagnosticsField, false)?.items ?? [];
    const counts = countBySeverity(items.map((i) => i.diag));
    const signature = JSON.stringify(counts);
    if (signature === this.signature) return;
    this.signature = signature;
    this.dom.replaceChildren();

    const total = counts.error + counts.warning + counts.weak + counts.grammar + counts.typo;
    if (total === 0) {
      const ok = h("button", "cm-lsp-inspection-ok", iconNode(Check, 14));
      ok.title = "No problems found";
      ok.onclick = () => showToolWindow("problems");
      this.dom.append(ok);
      return;
    }
    const summary = h("button", "cm-lsp-inspection-counts");
    const tips: string[] = [];
    for (const sev of ["error", "warning", "weak", "grammar", "typo"] as const) {
      if (!counts[sev]) continue;
      summary.append(h("span", `cm-lsp-count sev-${sev}`, iconNode(severityIcon[sev], 13), String(counts[sev])));
      tips.push(pluralize(counts[sev], severityLabel[sev].toLowerCase()));
    }
    summary.title = `${tips.join(", ")}\nClick to open the Problems tool window`;
    summary.onclick = () => showToolWindow("problems");
    const prev = h("button", "cm-lsp-inspection-nav", iconNode(ChevronUp, 14));
    prev.title = "Previous Highlighted Error (Shift+F2)";
    prev.onclick = () => gotoProblem(this.view, -1);
    const next = h("button", "cm-lsp-inspection-nav", iconNode(ChevronDown, 14));
    next.title = "Next Highlighted Error (F2)";
    next.onclick = () => gotoProblem(this.view, 1);
    this.dom.append(summary, prev, next);
  }

  destroy() {
    this.dom.remove();
  }
}

/** Feeds diagnostics published for `path` into the editor. */
function diagnosticsFeed(path: string) {
  return ViewPlugin.define((view) => {
    let alive = true;
    const apply = (diags: EditorDiagnostic[]) =>
      // Dispatching while the view is being constructed/updated is not allowed; defer.
      queueMicrotask(() => alive && view.dispatch({ effects: setDiagnosticsEffect.of(diags) }));
    const initial = diagnosticsStore[path];
    if (initial?.length) apply([...initial]);
    const off = onDiagnostics((p, diags) => {
      if (p === path) apply(diags);
    });
    return {
      destroy() {
        alive = false;
        off();
      },
    };
  });
}

export function lspDiagnostics(path: string): Extension {
  return [
    diagnosticsField,
    diagnosticsGutter,
    ViewPlugin.fromClass(InspectionWidget),
    diagnosticsFeed(path),
    keymap.of([
      { key: "F2", run: (v) => gotoProblem(v, 1) },
      { key: "Shift-F2", run: (v) => gotoProblem(v, -1) },
    ]),
  ];
}
