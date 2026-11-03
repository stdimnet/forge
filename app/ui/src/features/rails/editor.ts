/**
 * Rails in the editor, computed in Rust (`rails_goto`, `rails_complete`, `rails_marks`):
 *
 * - Go to Declaration (Ctrl+B, Ctrl+Click) on route helpers (`articles_path`), `render` /
 *   `layout` names, translation keys, Stimulus controllers, actions and targets, Turbo frame
 *   ids and the targets of config/routes.rb; anything else goes to Ruby LSP;
 * - completion of route helpers, partials, translation keys, model attributes, Stimulus
 *   identifiers / actions / targets and Turbo helpers and frames;
 * - gutter icons: action ↔ views and routes, view → action, partial → its renderers, model →
 *   table and migrations, associations, routes → actions, Stimulus controllers ↔ views;
 * - inspections (missing translations, unresolved partials, unknown Stimulus names) and
 *   folds of `t(".key")` with the text of the default locale.
 */
import { createEffect, createRoot, on } from "solid-js";
import { EditorState, Prec, RangeSetBuilder, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { Braces, Database, Globe, Languages, LayoutTemplate, Route, SquareArrowOutUpRight, SquareFunction, Table2, Zap } from "lucide-solid";
import { railsApi, type RailsFileMarks, type RailsFold, type RailsGutterMark, type RailsLocation } from "@/ipc/rails";
import type { EditorDiagnostic, Severity } from "@/ipc/lsp";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { publishDiagnostics } from "@/features/lsp/store";
import { registerCompletionKind } from "@/features/lsp/completion";
import { activeView } from "@/features/lsp/navigation";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { commands, registerCommand } from "@/state/commands";
import { getSetting } from "@/state/settings";
import { claimMouseGesture, editorMouseAction } from "@/keymap";
import { railsActive, reveal } from "./state";

const SOURCE = "rails";

registerCompletionKind("railsRoute", Route, "function");
registerCompletionKind("railsView", LayoutTemplate, "type");
registerCompletionKind("railsI18n", Languages, "constant");
registerCompletionKind("railsAttribute", Database, "property");
registerCompletionKind("railsStimulus", Zap, "type");
registerCompletionKind("railsAction", SquareFunction, "function");
registerCompletionKind("railsOther", Braces, "variable");

/** Files the Rails analysis looks at. */
export function railsApplies(path: string): boolean {
  if (/\.(erb|haml|slim|jbuilder|builder)$/i.test(path)) return true;
  if (/\.rb$/i.test(path)) return /^(app|config|lib|db|test|spec)\//.test(path);
  if (/\.html$/i.test(path)) return path.startsWith("app/views/");
  return /^app\/(javascript|frontend|assets\/javascripts)\/controllers\/.+[_-]controller\.[jt]s$/.test(path);
}

const isErb = (path: string) => /\.erb$/i.test(path);

/** The caret is in a string, or on a route helper (checked before asking Rust). */
export function mayReference(line: string, col: number): boolean {
  const before = line.slice(0, col);
  const quotes = (before.match(/"/g)?.length ?? 0) % 2 === 1 || (before.match(/'/g)?.length ?? 0) % 2 === 1;
  if (quotes) return true;
  const word = /\w*$/.exec(before)![0] + /^\w*/.exec(line.slice(col))![0];
  return /_(path|url)$/.test(word) || /^:\w+/.test(line.slice(before.length - /\w*$/.exec(before)![0].length - 1));
}

async function open(view: EditorView, pos: number, targets: RailsLocation[]): Promise<void> {
  if (targets.length === 1) await reveal(targets[0]);
  else
    chooseDeclarationAt(
      view,
      pos,
      targets.map((t) => ({ path: t.path, line: t.line, column: t.character })),
      { open: (t) => void reveal({ path: t.path, line: t.line, character: t.column ?? 0 }) },
    );
}

/** Go to what the reference at `pos` names; false when Rails has nothing there. */
export async function railsGoto(path: string, view: EditorView, pos: number): Promise<boolean> {
  if (!railsActive() || !railsApplies(path)) return false;
  const line = view.state.doc.lineAt(pos);
  if (!mayReference(line.text, pos - line.from)) return false;
  const targets = await railsApi.goto(path, view.state.doc.toString(), pos).catch(() => []);
  if (!targets.length) return false;
  await open(view, pos, targets);
  return true;
}

/** Ctrl+B reaches editors as the global `GotoDeclaration` action: Rails names first. */
export function wrapGotoDeclaration(): void {
  const previous = commands().get("GotoDeclaration");
  registerCommand({
    id: "GotoDeclaration",
    title: previous?.title ?? "Go to Declaration or Usages",
    category: previous?.category ?? "Navigate",
    run: async () => {
      const a = activeView();
      if (a && (await railsGoto(a.path, a.view, a.view.state.selection.main.head))) return;
      await previous?.run();
    },
  });
}

function mouseGoto(path: string): Extension {
  return Prec.high(
    EditorView.domEventHandlers({
      mousedown(e, view) {
        if (e.button !== 0 || editorMouseAction(e) !== "GotoDeclaration" || !railsActive()) return false;
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        if (pos == null) return false;
        const line = view.state.doc.lineAt(pos);
        if (!mayReference(line.text, pos - line.from)) return false;
        e.preventDefault();
        claimMouseGesture();
        view.dispatch({ selection: { anchor: pos } });
        void railsGoto(path, view, pos).then(async (done) => {
          if (done) return;
          await commands().get("GotoDeclaration")?.run();
        });
        return true;
      },
    }),
  );
}

const KINDS: Record<string, string> = {
  route: "railsRoute",
  partial: "railsView",
  template: "railsView",
  i18n: "railsI18n",
  attribute: "railsAttribute",
  association: "railsAttribute",
  scope: "railsAction",
  stimulus: "railsStimulus",
  action: "railsAction",
  target: "railsStimulus",
  turbo: "railsOther",
  frame: "railsOther",
};

/** Whether completion should ask Rust (in a string, after `.`, `:` or `(`, or a 2+ letter word). */
function worthAsking(before: string): boolean {
  if (/["'][^"'\n]*$/.test(before)) return true;
  if (/[.(:]\w*$/.test(before) || /,\s*:?\w*$/.test(before)) return true;
  return /[A-Za-z_]\w$/.test(before) || /[A-Za-z_]\w{2,}$/.test(before);
}

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!railsActive()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (!worthAsking(before)) return null;
    const answer = await railsApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return {
      from: answer.from,
      to: Math.max(answer.to, ctx.pos),
      options: answer.items.map((i) => ({
        label: i.label,
        apply: i.apply,
        detail: i.detail,
        type: KINDS[i.kind] ?? "railsOther",
        boost: i.kind === "route" ? 5 : 40,
      })),
      validFor: /^[\w./:#>-]*$/,
    };
  };
}

// ---- Marks: one request per text for the gutter, the problems and the folds ----------------

const last = new Map<string, { text: string; marks: Promise<RailsFileMarks | null> }>();

function marksFor(path: string, text: string): Promise<RailsFileMarks | null> {
  const hit = last.get(path);
  if (hit && hit.text === text) return hit.marks;
  const marks = railsApi.marks(path, text).catch(() => null);
  last.set(path, { text, marks });
  return marks;
}

const foldingOn = () => railsActive() && getSetting<boolean>("rails.i18n.folding", true) !== false;
const i18nInspectionOn = () => getSetting<boolean>("rails.i18n.inspection", true) !== false;

const SEVERITY: Record<string, Severity> = { error: "error", warning: "warning", info: "weak" };

function publish(path: string, marks: RailsFileMarks | null): void {
  const diags: EditorDiagnostic[] = (marks?.problems ?? [])
    .filter((p) => p.code !== "RailsI18nMissing" || i18nInspectionOn())
    .map((p) => ({
      range: { start: { line: p.line, character: p.start }, end: { line: p.line, character: p.end } },
      severity: SEVERITY[p.severity] ?? "warning",
      message: p.message,
      source: SOURCE,
      code: p.code,
    }));
  publishDiagnostics(path, diags, SOURCE);
}

const ICONS: Record<RailsGutterMark["kind"], typeof Globe> = {
  view: LayoutTemplate,
  route: Globe,
  controller: SquareArrowOutUpRight,
  partial: LayoutTemplate,
  table: Table2,
  model: SquareArrowOutUpRight,
  action: SquareArrowOutUpRight,
  stimulus: Zap,
};

registerLineMarkerProvider({
  appliesTo: railsApplies,
  watch: () => railsActive(),
  markers: async (path, text) => {
    if (!railsActive()) return [];
    const marks = await marksFor(path, text);
    return (marks?.gutter ?? []).map((m) => ({
      line: m.line,
      icon: ICONS[m.kind],
      title: m.tooltip,
      className: `marker-rails-${m.kind}`,
      onClick: (view: EditorView) => {
        const pos = view.state.doc.line(Math.min(m.line + 1, view.state.doc.lines)).from;
        void open(view, pos, m.targets);
      },
    }));
  },
});

// ---- Folds of translation calls --------------------------------------------------------------------

const setFolds = StateEffect.define<RailsFold[]>();
const refresh = StateEffect.define<null>();

const foldsField = StateField.define<RailsFold[]>({
  create: () => [],
  update(folds, tr) {
    for (const e of tr.effects) if (e.is(setFolds)) return e.value;
    if (!tr.docChanged) return folds;
    const out: RailsFold[] = [];
    for (const f of folds) {
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
    el.className = "cm-rails-translation-fold";
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
  ".cm-rails-translation-fold": {
    backgroundColor: "var(--bg-hover)",
    border: "1px solid var(--border-strong)",
    borderRadius: "3px",
    color: "var(--fg-muted)",
    padding: "0 3px",
    cursor: "pointer",
  },
});

/** Problems and folds after each pause in typing. */
function analysis(path: string): Extension {
  const plugin = ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = () => {
      const n = ++seq;
      if (!railsActive()) {
        publish(path, null);
        view.dispatch({ effects: setFolds.of([]) });
        return;
      }
      const text = view.state.doc.toString();
      void marksFor(path, text).then((m) => {
        if (n !== seq || view.state.doc.toString() !== text) return;
        publish(path, m);
        view.dispatch({ effects: setFolds.of(foldingOn() ? (m?.folds ?? []) : []) });
      });
    };
    timer = setTimeout(run, 50);
    const dispose = createRoot((d) => {
      createEffect(on([foldingOn, i18nInspectionOn, railsActive], () => queueMicrotask(() => view.dispatch({ effects: refresh.of(null) })), { defer: true }));
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
        last.delete(path);
        publishDiagnostics(path, [], SOURCE);
      },
    };
  });
  return [foldsField, foldView, foldTheme, plugin];
}

registerEditorExtension(({ path }): Extension | null => {
  if (!railsApplies(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  const exts: Extension[] = [EditorState.languageData.of(() => data), mouseGoto(path)];
  if (/\.(rb|erb|haml|slim|html)$/i.test(path)) exts.push(analysis(path));
  return exts;
});

/** For tests. */
export const __railsEditor = { isErb, worthAsking };
