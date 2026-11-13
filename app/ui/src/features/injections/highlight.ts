/**
 * Injected languages in the editor: where they are comes from Rust (`injections_find`,
 * in-process as WebAssembly right after an edit, else over IPC); until the answer lands the
 * known injections move with the text. Every injected range is colored by its language's
 * parser on each change (TypeScript, per keystroke) and gets the injected-fragment
 * background.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { Prec, RangeSetBuilder, StateEffect, StateField, type ChangeDesc, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { injectionsApi, type Injection, type InjectionSettings, type Span } from "@/ipc/injections";
import { getSetting } from "@/state/settings";
import { tokenize } from "./languages";

export const DISABLED_KEY = "editor.injections.disabled";
export const UNINJECTED_KEY = "editor.injections.uninjected";

/** Files above this size are not analysed. */
const MAX_CHARS = 1_000_000;

/** Parameters marked `#[Language]`, set by the PHP feature after indexing. */
const [injectionPlaces, setInjectionPlaces] = createSignal<NonNullable<InjectionSettings["places"]>>([]);
export { setInjectionPlaces };

export const injectionSettings = (): InjectionSettings => ({
  disabled: getSetting<string[]>(DISABLED_KEY, []) ?? [],
  uninjected: getSetting<string[]>(UNINJECTED_KEY, []) ?? [],
  places: injectionPlaces(),
});

/** Host files: PHP (not Blade) and JavaScript/TypeScript, as `Host::of_path` in Rust. */
export const isInjectionHost = (path: string): boolean =>
  !/\.blade\.php$/i.test(path) && /\.(php|phtml|inc|module|[cm]?js|jsx|[cm]?ts|tsx)$/i.test(path);

// ---------------------------------------------------------------- state

export const setInjections = StateEffect.define<Injection[]>();
/** Asks the plugin to analyse the document again (settings changed). */
export const refreshInjections = StateEffect.define<null>();

const mapSpan = (s: Span, ch: ChangeDesc): Span => ({ from: ch.mapPos(s.from, -1), to: ch.mapPos(s.to, 1) });

function mapInjection(i: Injection, ch: ChangeDesc): Injection {
  return {
    ...i,
    from: ch.mapPos(i.from, -1),
    to: ch.mapPos(i.to, 1),
    ranges: i.ranges.map((r) => mapSpan(r, ch)),
    literals: i.literals.map((l) => ({
      ...l,
      from: ch.mapPos(l.from, -1),
      to: ch.mapPos(l.to, 1),
      contentFrom: ch.mapPos(l.contentFrom, -1),
      contentTo: ch.mapPos(l.contentTo, 1),
    })),
    comment: i.comment ? mapSpan(i.comment, ch) : i.comment,
  };
}

/** The injections of the document, mapped through edits until Rust answers again. */
export const injectionsField = StateField.define<Injection[]>({
  create: () => [],
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setInjections)) return e.value;
    return tr.docChanged && value.length ? value.map((i) => mapInjection(i, tr.changes)) : value;
  },
});

/** The injection whose literals contain `pos` (inside the quotes, ends included). */
export function injectionAt(state: EditorState, pos: number): Injection | undefined {
  const list = state.field(injectionsField, false) ?? [];
  return list.find((i) => i.literals.some((l) => l.contentFrom <= pos && pos <= l.contentTo));
}

// ---------------------------------------------------------------- analysis

function analyse(view: EditorView, path: string, done: (list: Injection[]) => void): void {
  const text = view.state.doc.toString();
  if (text.length > MAX_CHARS) return done([]);
  const request = { text, path, ...injectionSettings() };
  let sync: Injection[] | undefined;
  try {
    sync = injectionsApi.findSync(request);
  } catch {
    sync = undefined;
  }
  if (sync) return done(sync);
  void injectionsApi
    .find(request)
    .then((list) => {
      if (view.state.doc.toString() === text) done(list);
    })
    .catch(() => undefined);
}

function analyser(path: string): Extension {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let destroyed = false;
    const run = () => {
      if (destroyed) return;
      analyse(view, path, (list) => {
        if (!destroyed) view.dispatch({ effects: setInjections.of(list) });
      });
    };
    timer = setTimeout(run, 0);
    // Settings | Editor | Language Injections and Un-inject Language apply at once.
    const dispose = createRoot((d) => {
      createEffect(
        on(
          () => JSON.stringify(injectionSettings()),
          () => queueMicrotask(() => !destroyed && view.dispatch({ effects: refreshInjections.of(null) })),
          { defer: true },
        ),
      );
      return d;
    });
    return {
      update(u: ViewUpdate) {
        if (!u.docChanged && !u.transactions.some((t) => t.effects.some((e) => e.is(refreshInjections)))) return;
        clearTimeout(timer);
        timer = setTimeout(run, u.docChanged ? 40 : 0);
      },
      destroy() {
        destroyed = true;
        clearTimeout(timer);
        dispose();
      },
    };
  });
}

// ---------------------------------------------------------------- decorations

const background = Decoration.mark({ class: "cm-injected" });
const marks = new Map<string, Decoration>();
const markOf = (cls: string) => {
  let m = marks.get(cls);
  if (!m) marks.set(cls, (m = Decoration.mark({ class: cls })));
  return m;
};

/** The ranges' text joined with a space (interpolations stand between them). */
function joined(doc: EditorState["doc"], ranges: Span[]): { text: string; starts: number[] } {
  let text = "";
  const starts: number[] = [];
  for (const r of ranges) {
    if (text) text += " ";
    starts.push(text.length);
    text += doc.sliceString(r.from, r.to);
  }
  return { text, starts };
}

function decorate(view: EditorView): DecorationSet {
  const state = view.state;
  const list = state.field(injectionsField, false) ?? [];
  if (!list.length) return Decoration.none;
  const { from: vFrom, to: vTo } = view.viewport;
  const out: { from: number; to: number; deco: Decoration; bg: boolean }[] = [];
  const len = state.doc.length;
  for (const inj of list) {
    if (inj.to < vFrom || inj.from > vTo) continue;
    const ranges = inj.ranges.filter((r) => r.to > r.from && r.to <= len);
    if (!ranges.length) continue;
    for (const r of ranges) out.push({ from: r.from, to: r.to, deco: background, bg: true });
    const { text, starts } = joined(state.doc, ranges);
    for (const tok of tokenize(inj.language, text)) {
      // Split tokens that span the space between two ranges.
      ranges.forEach((r, k) => {
        const s = starts[k];
        const e = s + (r.to - r.from);
        const from = Math.max(tok.from, s);
        const to = Math.min(tok.to, e);
        if (to > from) out.push({ from: r.from + from - s, to: r.from + to - s, deco: markOf(tok.cls), bg: false });
      });
    }
  }
  out.sort((a, b) => a.from - b.from || Number(b.bg) - Number(a.bg));
  const builder = new RangeSetBuilder<Decoration>();
  for (const d of out) builder.add(d.from, d.to, d.deco);
  return builder.finish();
}

const decorations = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view);
    }
    update(u: ViewUpdate) {
      const changed = u.startState.field(injectionsField, false) !== u.state.field(injectionsField, false);
      if (changed || u.docChanged || u.viewportChanged) this.decorations = decorate(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);

/** Injected language highlighting for a PHP or JavaScript/TypeScript editor. */
export function injectionHighlighting(path: string): Extension {
  // Above the host language's own highlighting, so the injected colors win.
  return [injectionsField, analyser(path), Prec.high(decorations)];
}
