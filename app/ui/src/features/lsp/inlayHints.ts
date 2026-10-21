/**
 * Inlay hints (Settings | Editor | Inlay Hints): parameter names at call sites and inferred
 * types the language servers report, drawn inline like PHPStorm's. Rust asks the servers,
 * groups the hints and applies the settings (`lsp_inlay_hints`); this file requests them for
 * the visible lines after edits and scrolling and draws them.
 */
import { StateEffect, StateField, type Range } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { lspApi, type EditorInlayHint } from "@/ipc/lsp";
import { languageIdFor } from "@/editor/languages";
import { settingsState } from "@/state/settings";
import { syncNow } from "./sync";
import { fromLsp } from "./util";

const REFRESH_DELAY_MS = 400;

class HintWidget extends WidgetType {
  constructor(readonly hint: EditorInlayHint) {
    super();
  }
  eq(other: HintWidget): boolean {
    return other.hint.label === this.hint.label && other.hint.group === this.hint.group;
  }
  toDOM(): HTMLElement {
    const el = document.createElement("span");
    el.className = `cm-inlay-hint cm-inlay-${this.hint.group}`;
    el.classList.toggle("pad-left", this.hint.paddingLeft);
    el.classList.toggle("pad-right", this.hint.paddingRight);
    el.textContent = this.hint.label;
    if (this.hint.tooltip) el.title = this.hint.tooltip;
    return el;
  }
  ignoreEvent(): boolean {
    return false;
  }
}

const setHints = StateEffect.define<DecorationSet>();

const hintsField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setHints)) return e.value;
    return tr.docChanged ? value.map(tr.changes) : value;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** Bumped when an Inlay Hints setting changes, so open editors ask again. */
const [settingsRevision, setSettingsRevision] = createSignal(0);
createRoot(() =>
  createEffect(
    on(
      () => JSON.stringify(Object.entries(settingsState.values).filter(([k]) => k.startsWith("editor.inlayHints."))),
      () => setSettingsRevision((n) => n + 1),
      { defer: true },
    ),
  ),
);

function decorations(view: EditorView, hints: EditorInlayHint[]): DecorationSet {
  const doc = view.state.doc;
  const ranges: Range<Decoration>[] = hints
    .filter((h) => h.line < doc.lines)
    .map((h) => Decoration.widget({ widget: new HintWidget(h), side: h.group === "parameterName" ? 1 : -1 }).range(fromLsp(doc, h)));
  return Decoration.set(ranges, true);
}

export function lspInlayHints(path: string) {
  return [
    hintsField,
    ViewPlugin.define((view) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      let seq = 0;
      const refresh = async () => {
        const n = ++seq;
        const { from, to } = view.viewport;
        const doc = view.state.doc;
        const text = doc.toString();
        await syncNow(path);
        const hints = await lspApi
          .inlayHints(path, doc.lineAt(from).number - 1, doc.lineAt(to).number - 1, text, languageIdFor(path, text) ?? null)
          .catch(() => [] as EditorInlayHint[]);
        // Drop answers for an older document or viewport.
        if (n !== seq || view.state.doc.toString() !== text) return;
        view.dispatch({ effects: setHints.of(decorations(view, hints)) });
      };
      const schedule = (ms = REFRESH_DELAY_MS) => {
        clearTimeout(timer);
        timer = setTimeout(() => void refresh(), ms);
      };
      schedule(0);
      const dispose = createRoot((d) => {
        createEffect(on(settingsRevision, () => schedule(0), { defer: true }));
        return d;
      });
      return {
        update(u: ViewUpdate) {
          if (u.docChanged || u.viewportChanged) schedule();
        },
        destroy() {
          clearTimeout(timer);
          seq++;
          dispose();
        },
      };
    }),
    EditorView.baseTheme({
      ".cm-inlay-hint": {
        display: "inline-block",
        padding: "0 4px",
        margin: "0 1px",
        borderRadius: "4px",
        fontSize: "0.86em",
        lineHeight: "1.3",
        color: "var(--fg-muted)",
        background: "var(--bg-hover)",
        verticalAlign: "baseline",
      },
      ".cm-inlay-hint.pad-left": { marginLeft: "4px" },
      ".cm-inlay-hint.pad-right": { marginRight: "4px" },
    }),
  ];
}
