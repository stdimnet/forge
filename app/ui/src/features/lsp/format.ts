/** Reformat Code (Ctrl+Alt+L) through the language server's document formatting. */
import type { ChangeSpec, Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { lspApi, type TextEdit } from "@/ipc/lsp";
import { syncNow } from "./sync";
import { fromLsp } from "./util";
import { editor } from "@/state/editor";

/** Apply LSP text edits (positions relative to the current document) in one transaction. */
export function applyTextEdits(view: EditorView, edits: TextEdit[], userEvent = "format"): void {
  if (!edits.length) return;
  const doc = view.state.doc;
  const changes: ChangeSpec[] = edits
    .map((e) => ({ from: fromLsp(doc, e.range.start), to: fromLsp(doc, e.range.end), insert: e.newText }))
    .sort((a, b) => a.from - b.from);
  view.dispatch({ changes, userEvent, scrollIntoView: false });
}

/**
 * A formatter that a language pack puts before the language server (StyLua for Lua).
 * Returns true when it handled the file.
 */
export type FormatProvider = (path: string, view: EditorView) => Promise<boolean>;

const formatProviders: FormatProvider[] = [];

export function registerFormatProvider(provider: FormatProvider): void {
  formatProviders.push(provider);
}

/** A step after the language server's formatting (PHP blank lines from the code style). */
export type FormatPostProcessor = (path: string, view: EditorView) => Promise<void>;

const postProcessors: FormatPostProcessor[] = [];

export function registerFormatPostProcessor(processor: FormatPostProcessor): void {
  postProcessors.push(processor);
}

/** Replace the document with `text` as one change of the part that differs. */
export function replaceChangedPart(view: EditorView, text: string, userEvent = "format"): void {
  const old = view.state.doc.toString();
  if (old === text) return;
  let start = 0;
  const max = Math.min(old.length, text.length);
  while (start < max && old[start] === text[start]) start++;
  let end = 0;
  while (end < max - start && old[old.length - 1 - end] === text[text.length - 1 - end]) end++;
  view.dispatch({
    changes: { from: start, to: old.length - end, insert: text.slice(start, text.length - end) },
    userEvent,
    scrollIntoView: false,
  });
}

export async function reformat(path: string, view: EditorView): Promise<void> {
  for (const provider of formatProviders) {
    if (await provider(path, view).catch(() => false)) return;
  }
  await syncNow(path);
  // Indent options from the code style (or EditorConfig / the detected indent).
  const indent = editor.tabs.find((t) => t.path === path)?.format?.indent;
  const edits = await lspApi.format(path, indent?.size ?? 4, !(indent?.useTabs ?? false)).catch(() => [] as TextEdit[]);
  applyTextEdits(view, edits);
  for (const processor of postProcessors) await processor(path, view).catch(() => undefined);
}

export function lspFormat(path: string): Extension {
  return keymap.of([{ key: "Mod-Alt-l", run: (view) => (void reformat(path, view), true) }]);
}
