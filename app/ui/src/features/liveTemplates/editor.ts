/**
 * Editor side of live templates: the expand key (Tab, Space or Enter after an abbreviation),
 * live templates in code completion, applying an expansion as a CodeMirror snippet (Tab and
 * Shift+Tab move between the variables, Escape leaves them) and the highlight of the
 * statement Unwrap/Remove would change.
 *
 * Expand keys are CodeMirror editor keys like Tab's indentation (PHPStorm's native editor
 * actions); whether a key expands is decided in Rust, which runs in-process as WebAssembly
 * because the key handler must answer at once.
 */
import { EditorState, Prec, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, keymap, type DecorationSet } from "@codemirror/view";
import { snippet, type Completion, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { getSetting } from "@/state/settings";
import { liveTemplatesApi, type Expansion } from "@/ipc/liveTemplates";
import { docRequest, envFor, EXPAND_KEY, toSnippet } from "./model";

/** Insert an expansion; its variables become snippet fields. */
export function applyExpansion(view: EditorView, e: Expansion): void {
  snippet(toSnippet(e))(view, null, e.from, e.to);
  view.focus();
}

function expandRequest(path: string, view: EditorView, templateId?: string) {
  return { ...docRequest(path, view), offset: view.state.selection.main.head, templateId, env: envFor(view) };
}

/** Expand the abbreviation before the caret; false when there is none (the key acts as usual). */
export function expandAbbreviation(path: string, view: EditorView): boolean {
  const sel = view.state.selection;
  if (sel.ranges.length > 1 || !sel.main.empty) return false;
  const head = sel.main.head;
  // Cheap check first: only a word right before the caret can be an abbreviation.
  if (!/[\w]$/.test(view.state.sliceDoc(Math.max(0, head - 1), head))) return false;
  const e = liveTemplatesApi.expandSync(expandRequest(path, view));
  if (!e) return false;
  applyExpansion(view, e);
  return true;
}

/** Expand a chosen template at the caret (Insert Live Template, completion). */
export async function expandTemplate(path: string, view: EditorView, templateId: string): Promise<void> {
  const request = expandRequest(path, view, templateId);
  const e = liveTemplatesApi.expandSync(request) ?? (await liveTemplatesApi.expand(request));
  if (e) applyExpansion(view, e);
}

const expandKey = (path: string, key: "tab" | "space" | "enter") => (view: EditorView) =>
  getSetting<string>(EXPAND_KEY, "tab") === key && expandAbbreviation(path, view);

/** Live templates in the completion list, as PHPStorm shows them next to code items. */
function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const word = ctx.matchBefore(/\w+/);
    if (!word && !ctx.explicit) return null;
    const view = ctx.view;
    if (!view) return null;
    const request = { ...docRequest(path, view), offset: ctx.pos };
    const result = await liveTemplatesApi.applicable(request).catch(() => null);
    if (!result?.items.length || ctx.aborted) return null;
    const options: Completion[] = result.items.map((t) => ({
      label: t.abbreviation,
      detail: t.description,
      type: "keyword",
      boost: -10,
      apply: (v: EditorView) => void expandTemplate(path, v, t.id),
    }));
    return { from: word?.from ?? ctx.pos, options, validFor: /^\w*$/ };
  };
}

/** Highlight of the statement an Unwrap/Remove entry changes: removed parts and the kept body. */
export const setUnwrapHighlight = StateEffect.define<{ from: number; to: number; keep?: [number, number] } | null>();

const removedMark = Decoration.mark({ class: "lt-unwrap-removed" });
const keptMark = Decoration.mark({ class: "lt-unwrap-kept" });

const unwrapHighlight = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    for (const e of tr.effects) {
      if (!e.is(setUnwrapHighlight)) continue;
      const h = e.value;
      if (!h || h.from >= h.to) return Decoration.none;
      const ranges = h.keep
        ? [
            h.from < h.keep[0] ? removedMark.range(h.from, h.keep[0]) : null,
            h.keep[0] < h.keep[1] ? keptMark.range(h.keep[0], h.keep[1]) : null,
            h.keep[1] < h.to ? removedMark.range(h.keep[1], h.to) : null,
          ]
        : [removedMark.range(h.from, h.to)];
      return Decoration.set(ranges.filter((r) => r !== null));
    }
    return tr.docChanged ? Decoration.none : deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

export function liveTemplatesExtension(path: string): Extension {
  return [
    Prec.high(
      keymap.of([
        { key: "Tab", run: expandKey(path, "tab") },
        { key: "Space", run: expandKey(path, "space") },
        { key: "Enter", run: expandKey(path, "enter") },
      ]),
    ),
    EditorState.languageData.of(() => [{ autocomplete: completionSource(path) }]),
    unwrapHighlight,
  ];
}
