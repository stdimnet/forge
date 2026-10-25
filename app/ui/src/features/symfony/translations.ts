/**
 * Symfony translations in PHP and Twig editors, like PHPStorm: translation keys fold to the
 * text of the default locale (the key shows while the caret is on it, a click opens it),
 * missing keys and locales are reported as warnings / weak warnings, and Alt+Enter offers
 * "Create translation key", which adds the key to the domain's catalogues. Everything is
 * computed in Rust (`symfony_translation_analysis`, `symfony_create_translation`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { RangeSetBuilder, StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { symfonyApi, type TranslationFold, type TranslationProblem } from "@/ipc/symfony";
import type { ContextAction } from "@/ipc/codeintel";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { registerEditorExtension } from "@/editor/extensions";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { publishDiagnostics } from "@/features/lsp/store";
import { toLsp } from "@/features/lsp/util";
import { revealSymbol } from "@/features/php/editorViews";
import { showBalloon } from "@/keymap";
import { getSetting } from "@/state/settings";
import { symfonyActive } from "./store";

const SOURCE = "symfony-translations";

const applies = (path: string) => /\.twig$/i.test(path) || (/\.php$/i.test(path) && !path.endsWith(".blade.php"));

const foldingOn = () => symfonyActive() && getSetting<boolean>("symfony.translations.folding", true) !== false;
const inspectionOn = () => symfonyActive() && getSetting<boolean>("symfony.translations.inspection", true) !== false;

/** Problems per open file, for Alt+Enter. */
const problemsByPath = new Map<string, TranslationProblem[]>();

// ---------------------------------------------------------------- folds

const setFolds = StateEffect.define<TranslationFold[]>();
const refresh = StateEffect.define<null>();

const foldsField = StateField.define<TranslationFold[]>({
  create: () => [],
  update(folds, tr) {
    for (const e of tr.effects) if (e.is(setFolds)) return e.value;
    if (!tr.docChanged) return folds;
    const out: TranslationFold[] = [];
    for (const f of folds) {
      // Drop folds whose text was edited.
      if (tr.changes.touchesRange(f.from, f.to)) continue;
      out.push({ ...f, from: tr.changes.mapPos(f.from), to: tr.changes.mapPos(f.to) });
    }
    return out;
  },
});

class TranslationWidget extends WidgetType {
  constructor(
    readonly text: string,
    readonly key: string,
    readonly pos: number,
  ) {
    super();
  }
  eq(other: TranslationWidget): boolean {
    return other.text === this.text && other.key === this.key && other.pos === this.pos;
  }
  toDOM(view: EditorView): HTMLElement {
    const el = document.createElement("span");
    el.className = "cm-sf-translation-fold";
    el.textContent = this.text;
    el.title = `${this.key} (click to show the key)`;
    el.onmousedown = (e) => {
      e.preventDefault();
      view.dispatch({ selection: { anchor: this.pos } });
      view.focus();
    };
    return el;
  }
  ignoreEvent(): boolean {
    return false;
  }
}

function foldDecorations(state: EditorState): DecorationSet {
  if (!foldingOn()) return Decoration.none;
  const sel = state.selection.main;
  const b = new RangeSetBuilder<Decoration>();
  for (const f of [...state.field(foldsField)].sort((a, c) => a.from - c.from)) {
    if (f.to > state.doc.length || f.to <= f.from) continue;
    // The key shows while the caret or selection touches it.
    if (sel.from <= f.to && sel.to >= f.from) continue;
    if (state.doc.lineAt(f.from).number !== state.doc.lineAt(f.to).number) continue;
    b.add(f.from, f.to, Decoration.replace({ widget: new TranslationWidget(f.text, f.key, f.from + 1) }));
  }
  return b.finish();
}

const foldView = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = foldDecorations(view.state);
    }
    update(u: ViewUpdate) {
      const changed = u.transactions.some((t) => t.effects.some((e) => e.is(setFolds) || e.is(refresh)));
      if (u.docChanged || u.selectionSet || changed) this.decorations = foldDecorations(u.state);
    }
  },
  { decorations: (v) => v.decorations },
);

const foldTheme = EditorView.baseTheme({
  ".cm-sf-translation-fold": {
    backgroundColor: "var(--bg-hover)",
    border: "1px solid var(--border-strong)",
    borderRadius: "3px",
    color: "var(--fg-muted)",
    padding: "0 3px",
    cursor: "pointer",
  },
});

// ---------------------------------------------------------------- analysis

function toDiagnostics(state: EditorState, problems: TranslationProblem[]): EditorDiagnostic[] {
  const doc = state.doc;
  return problems
    .filter((p) => p.to <= doc.length)
    .map((p) => ({
      range: { start: toLsp(doc, p.from), end: toLsp(doc, p.to) },
      severity: p.severity,
      message: p.message,
      source: "Symfony",
      code: "SymfonyTranslation",
      data: p,
    }));
}

function analysis(path: string): Extension {
  const plugin = ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = () => {
      const n = ++seq;
      if (!foldingOn() && !inspectionOn()) {
        problemsByPath.delete(path);
        publishDiagnostics(path, [], SOURCE);
        view.dispatch({ effects: setFolds.of([]) });
        return;
      }
      const text = view.state.doc.toString();
      void symfonyApi
        .translationAnalysis(path, text)
        .then((a) => {
          if (n !== seq || view.state.doc.toString() !== text) return;
          const problems = inspectionOn() ? a.problems : [];
          problemsByPath.set(path, problems);
          publishDiagnostics(path, toDiagnostics(view.state, problems), SOURCE);
          view.dispatch({ effects: setFolds.of(foldingOn() ? a.folds : []) });
        })
        .catch(() => {});
    };
    timer = setTimeout(run, 50);
    const dispose = createRoot((d) => {
      createEffect(on([foldingOn, inspectionOn], () => queueMicrotask(() => view.dispatch({ effects: refresh.of(null) })), { defer: true }));
      return d;
    });
    return {
      update(u: ViewUpdate) {
        const again = u.transactions.some((t) => t.effects.some((e) => e.is(refresh)));
        if (!u.docChanged && !again) return;
        clearTimeout(timer);
        timer = setTimeout(run, again ? 0 : 500);
      },
      destroy() {
        clearTimeout(timer);
        seq++;
        dispose();
        problemsByPath.delete(path);
        publishDiagnostics(path, [], SOURCE);
      },
    };
  });
  return [foldsField, foldView, foldTheme, plugin];
}

registerEditorExtension(({ path }) => (applies(path) ? analysis(path) : null));

// ---------------------------------------------------------------- Create translation key

interface CreateData {
  domain: string;
  key: string;
  locales: string[];
}

registerContextActionProvider(async (path, view) => {
  if (!applies(path) || !symfonyActive()) return [];
  const pos = view.state.selection.main.head;
  const p = (problemsByPath.get(path) ?? []).find((x) => x.from <= pos && pos <= x.to);
  if (!p) return [];
  const action: ContextAction = {
    title: p.severity === "warning" ? `Create translation key '${p.key}'` : `Create translation key '${p.key}' for ${p.locales.join(", ")}`,
    kind: "quickFix",
    source: "symfony",
    isPreferred: true,
    needsResolve: false,
    action: "Symfony.CreateTranslationKey",
    data: { domain: p.domain, key: p.key, locales: p.locales } satisfies CreateData,
  };
  return [action];
});

registerContextActionHandler("Symfony.CreateTranslationKey", async (data, ctx) => {
  const d = data as CreateData;
  try {
    const created = await symfonyApi.createTranslation(d.domain, d.key, d.locales);
    const first = created[0];
    showBalloon(
      `Translation key '${d.key}' created`,
      created.map((l) => l.path).join(", "),
      4000,
      first ? [{ label: "Open", run: () => void revealSymbol(first.path, { line: first.line }) }] : undefined,
      { group: "Symfony" },
    );
  } catch (e) {
    showBalloon("Could not create the translation key", String(e), 5000, undefined, { group: "Symfony", type: "error" });
  }
  ctx.view.dispatch({ effects: refresh.of(null) });
});
