/** Reformat Code (Ctrl+Alt+L) through the language server's document formatting. */
import type { ChangeSpec } from "@codemirror/state";
import { isolateHistory } from "@codemirror/commands";
import { EditorView } from "@codemirror/view";
import { lspApi, type TextEdit } from "@/ipc/lsp";
import { syncNow } from "./sync";
import { fromLsp } from "./util";
import { editor } from "@/state/editor";
import { languageIdFor } from "@/editor/languages";

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
 * A formatter that a language pack puts before the language server (StyLua for Lua): the
 * formatted text, or null when it does not handle the file.
 */
export type FormatProvider = (path: string, text: string) => Promise<string | null>;

const formatProviders: FormatProvider[] = [];

export function registerFormatProvider(provider: FormatProvider): void {
  formatProviders.push(provider);
}

/**
 * A step after the formatter (PHP blank lines, the code style pass): gets the formatted text
 * and the text before formatting.
 */
export type FormatPostProcessor = (path: string, text: string, original: string) => Promise<string>;

const postProcessors: { processor: FormatPostProcessor; order: number }[] = [];

/** Post-processors run by ascending `order` (default 0), then in registration order. */
export function registerFormatPostProcessor(processor: FormatPostProcessor, order = 0): void {
  postProcessors.push({ processor, order });
  postProcessors.sort((a, b) => a.order - b.order);
}

/**
 * Replace the document with `text` as one change of the part that differs. `isolate` keeps
 * it an undo step of its own (never merged with typing around it).
 */
export function replaceChangedPart(view: EditorView, text: string, userEvent = "format", isolate = false): void {
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
    annotations: isolate ? isolateHistory.of("full") : undefined,
  });
}

/** The language server's formatting of the document, applied to its text. */
async function serverFormatted(path: string, view: EditorView, text: string): Promise<string> {
  await syncNow(path);
  // Indent options from the code style (or EditorConfig / the detected indent); the other
  // code style options reach the server through its settings (by file type).
  const indent = editor.tabs.find((t) => t.path === path)?.format?.indent;
  const languageId = languageIdFor(path, text) ?? null;
  const edits = await lspApi
    .format(path, indent?.size ?? 4, !(indent?.useTabs ?? false), languageId)
    .catch(() => [] as TextEdit[]);
  if (!edits.length || view.state.doc.toString() !== text) return text;
  const doc = view.state.doc;
  const changes: ChangeSpec[] = edits
    .map((e) => ({ from: fromLsp(doc, e.range.start), to: fromLsp(doc, e.range.end), insert: e.newText }))
    .sort((a, b) => a.from - b.from);
  return view.state.update({ changes }).state.doc.toString();
}

/**
 * The reformatted text of the document without changing it: a language pack's formatter,
 * else the language server, then the post-processors (the code style options the formatter
 * does not know). Used by Reformat Code and by the Actions on Save (which apply all their
 * changes as one edit).
 */
export async function formattedText(path: string, view: EditorView): Promise<string> {
  const text = view.state.doc.toString();
  let out: string | null = null;
  for (const provider of formatProviders) {
    out = await provider(path, text).catch(() => null);
    if (out !== null) break;
  }
  let result: string = out ?? (await serverFormatted(path, view, text));
  for (const { processor } of postProcessors) {
    const before: string = result;
    result = await processor(path, before, text).catch(() => before);
  }
  return result;
}

export async function reformat(path: string, view: EditorView): Promise<void> {
  const before = view.state.doc.toString();
  const out = await formattedText(path, view);
  // Only when nobody typed meanwhile.
  if (view.state.doc.toString() === before) replaceChangedPart(view, out);
}
