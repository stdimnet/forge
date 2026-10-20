/**
 * DQL language injection in PHP: highlighting of `createQuery()` and QueryBuilder strings,
 * completion of entities/aliases/fields/keywords, Ctrl+B to entity fields, unknown-field
 * warnings and entity <-> repository line markers.
 */
import { createEffect, createRoot, on } from "solid-js";
import { RangeSetBuilder, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { Database, Table2 } from "lucide-solid";
import { doctrineApi, type DqlItem, type DqlItemKind } from "@/ipc/doctrine";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { revealSymbol } from "@/features/php/editorViews";
import { getSetting } from "@/state/settings";
import { dqlSourceSpans, findDqlSites, sourceToDql } from "./dql";
import { doctrineActive } from "./store";

const isPhp = (path: string) => /\.php$/i.test(path) && !path.endsWith(".blade.php");

const injectionOn = () => doctrineActive() && getSetting<boolean>("doctrine.dqlInjection", true);

// ---------------------------------------------------------------- highlighting

const marks = {
  keyword: Decoration.mark({ class: "cm-dql-keyword" }),
  function: Decoration.mark({ class: "cm-dql-function" }),
  entity: Decoration.mark({ class: "cm-dql-entity" }),
  alias: Decoration.mark({ class: "cm-dql-alias" }),
  field: Decoration.mark({ class: "cm-dql-field" }),
  parameter: Decoration.mark({ class: "cm-dql-parameter" }),
  string: Decoration.mark({ class: "cm-dql-string" }),
  number: Decoration.mark({ class: "cm-dql-number" }),
};
const injected = Decoration.mark({ class: "cm-dql-injected" });

function buildDecorations(state: EditorState): DecorationSet {
  if (!injectionOn()) return Decoration.none;
  const { sites, spans } = dqlSourceSpans(state.doc.toString());
  const all: { from: number; to: number; deco: Decoration }[] = [];
  for (const site of sites) for (const s of site.segments) if (s.len > 0) all.push({ from: s.src, to: s.src + s.len, deco: injected });
  for (const sp of spans) all.push({ from: sp.from, to: sp.to, deco: marks[sp.kind] });
  all.sort((a, b) => a.from - b.from || Number(b.deco === injected) - Number(a.deco === injected));
  const builder = new RangeSetBuilder<Decoration>();
  for (const d of all) if (d.to <= state.doc.length) builder.add(d.from, d.to, d.deco);
  return builder.finish();
}

const highlighter = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    dispose: () => void;
    constructor(view: EditorView) {
      this.decorations = buildDecorations(view.state);
      // The plugin becomes active once the project type is detected; refresh then.
      this.dispose = createRoot((dispose) => {
        createEffect(on(injectionOn, () => queueMicrotask(() => view.dispatch({ effects: refreshDql.of(null) })), { defer: true }));
        return dispose;
      });
    }
    destroy() {
      this.dispose();
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.transactions.some((t) => t.effects.some((e) => e.is(refreshDql)))) {
        this.decorations = buildDecorations(u.state);
      }
    }
  },
  { decorations: (v) => v.decorations },
);

/** Re-run highlighting once the project is detected (the plugin may become active later). */
const refreshDql = StateEffect.define<null>();

// ---------------------------------------------------------------- diagnostics

const setProblems = StateEffect.define<DecorationSet>();
const problemsField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    for (const e of tr.effects) if (e.is(setProblems)) return e.value;
    return tr.docChanged ? deco.map(tr.changes) : deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

function diagnostics(path: string): Extension {
  const plugin = ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = () => {
      const n = ++seq;
      if (!injectionOn() || !getSetting<boolean>("doctrine.dqlInspections", true) || !findDqlSites(view.state.doc.toString()).length) {
        view.dispatch({ effects: setProblems.of(Decoration.none) });
        return;
      }
      void doctrineApi
        .diagnostics(path, view.state.doc.toString())
        .then((list) => {
          if (n !== seq) return;
          const b = new RangeSetBuilder<Decoration>();
          for (const d of [...list].sort((a, c) => a.start - c.start)) {
            if (d.end <= view.state.doc.length && d.end > d.start) {
              b.add(d.start, d.end, Decoration.mark({ class: "cm-dql-problem", attributes: { title: d.message } }));
            }
          }
          view.dispatch({ effects: setProblems.of(b.finish()) });
        })
        .catch(() => {});
    };
    timer = setTimeout(run, 300);
    return {
      update(u) {
        if (!u.docChanged && !u.transactions.some((t) => t.effects.some((e) => e.is(refreshDql)))) return;
        clearTimeout(timer);
        timer = setTimeout(run, 700);
      },
      destroy() {
        clearTimeout(timer);
        seq++;
      },
    };
  });
  return [problemsField, plugin];
}

// ---------------------------------------------------------------- completion

const TYPE: Record<DqlItemKind, string> = {
  keyword: "keyword",
  function: "function",
  entity: "class",
  alias: "variable",
  field: "field",
  association: "interface",
  embedded: "struct",
};

const BOOST: Record<DqlItemKind, number> = { alias: 99, field: 95, association: 95, embedded: 95, entity: 90, function: 60, keyword: 50 };

function toOption(item: DqlItem): Completion {
  return {
    label: item.label,
    type: TYPE[item.kind],
    detail: item.detail,
    apply: item.apply ?? item.label,
    boost: BOOST[item.kind],
  };
}

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!injectionOn() || !getSetting<boolean>("doctrine.dqlCompletion", true)) return null;
    const text = ctx.state.doc.toString();
    // Only inside DQL strings; checked locally before asking the backend.
    if (!findDqlSites(text).some((s) => sourceToDql(s, ctx.pos) != null)) return null;
    const word = ctx.matchBefore(/[\w\\.]*$/);
    if (!ctx.explicit && !word?.text && !/\s$/.test(ctx.state.sliceDoc(ctx.pos - 1, ctx.pos))) return null;
    const res = await doctrineApi.complete(path, text, ctx.pos).catch(() => null);
    if (!res || ctx.aborted || !res.items.length) return null;
    return { from: res.from, options: res.items.map(toOption), validFor: /^[\w\\]*$/ };
  };
}

// ---------------------------------------------------------------- registration

registerEditorExtension(({ path }) => {
  if (!isPhp(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return [highlighter, diagnostics(path), EditorState.languageData.of(() => data)];
});

registerDeclarationProvider(async (path, view, pos) => {
  if (!isPhp(path) || !injectionOn()) return false;
  const text = view.state.doc.toString();
  if (!findDqlSites(text).some((s) => sourceToDql(s, pos) != null)) return false;
  const t = (await doctrineApi.goto(path, text, pos))[0];
  if (!t) return false;
  await revealSymbol(t.path, { line: t.line });
  return true;
});

registerLineMarkerProvider({
  appliesTo: isPhp,
  watch: doctrineActive,
  markers: async (path, text) => {
    if (!doctrineActive()) return [];
    const marks = await doctrineApi.gutter(path, text);
    return marks.map((m) => ({
      line: m.line,
      icon: m.kind === "entity" ? Database : Table2,
      title: m.title,
      className: m.kind === "entity" ? "marker-entity" : "marker-repository",
      onClick: () => {
        const t = m.targets[0];
        if (t) void revealSymbol(t.path, { line: t.line });
      },
    }));
  },
});
