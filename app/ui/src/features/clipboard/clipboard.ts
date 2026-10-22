/**
 * Copy and paste with multiple carets, and recording of copies for Paste from History.
 *
 * Copies and cuts in Forge go into the session clipboard history (Rust,
 * `forge_editing::clipboard`). A copy with N carets remembers each caret's text, so pasting
 * it with N carets puts one part at each caret, like PHPStorm; with another number of carets
 * the whole text is pasted (or one line per caret when the line count matches).
 */
import { EditorSelection, type EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { clipboardApi, type ClipEntry } from "@/ipc/clipboard";
import { getSetting } from "@/state/settings";

export const MAX_CONTENTS_KEY = "editor.clipboard.maxContents";

/** What a copy or cut takes from an editor: the selections, or whole lines without one. */
export interface Copied {
  text: string;
  /** Text per caret (or per line for line-wise copies). */
  carets: string[];
  ranges: { from: number; to: number }[];
  /** Whole lines (no selection): pasted above the caret line. */
  linewise: boolean;
}

/** The same choice CodeMirror's own copy makes, with the part of every caret. */
export function copiedFrom(state: EditorState): Copied {
  const carets: string[] = [];
  const ranges: { from: number; to: number }[] = [];
  for (const r of state.selection.ranges) {
    if (r.empty) continue;
    carets.push(state.sliceDoc(r.from, r.to));
    ranges.push({ from: r.from, to: r.to });
  }
  if (carets.length) return { text: carets.join(state.lineBreak), carets, ranges, linewise: false };
  let upto = -1;
  for (const r of state.selection.ranges) {
    const line = state.doc.lineAt(r.from);
    if (line.number > upto) {
      carets.push(line.text);
      ranges.push({ from: line.from, to: Math.min(state.doc.length, line.to + 1) });
    }
    upto = line.number;
  }
  return { text: carets.join(state.lineBreak), carets, ranges, linewise: true };
}

const normalize = (text: string) => text.replace(/\r\n?/g, "\n");

/** The last copy made in Forge (its caret parts are not on the system clipboard). */
let last: ClipEntry | null = null;

export const lastCopy = (): ClipEntry | null => last;

const copyListeners: ((text: string) => void)[] = [];

/** Follow the texts copied in Forge (e.g. Vim's `+` register mirrors them). */
export function onCopy(listener: (text: string) => void): void {
  copyListeners.push(listener);
}

/** An entry pasted from the history is the current clipboard content again. */
export function setLastCopy(entry: ClipEntry): void {
  last = { text: normalize(entry.text), carets: entry.carets ?? [] };
  for (const l of copyListeners) l(last.text);
}

/** Remember a copied or cut text (system clipboard writes are the caller's job). */
export function recordCopy(text: string, carets: string[] = []): void {
  if (!text) return;
  const entry: ClipEntry = { text: normalize(text), carets: carets.length > 1 ? carets.map(normalize) : [] };
  last = entry;
  for (const l of copyListeners) l(entry.text);
  void clipboardApi.add(entry, getSetting<number>(MAX_CONTENTS_KEY, 20)).catch(() => undefined);
}

/** The parts to put at the carets: the remembered parts of `text` when the caret count matches. */
function caretParts(text: string, carets: number, parts?: string[]): string[] | null {
  const known = parts ?? (last && last.text === text ? last.carets : undefined);
  if (carets > 1 && known && known.length === carets) return known;
  return null;
}

/**
 * The transaction that pastes `text`: one part per caret when the copy had as many carets
 * (or the text as many lines), else the whole text at every caret.
 */
export function pasteSpec(state: EditorState, input: string, parts?: string[]) {
  const text = normalize(input);
  const count = state.selection.ranges.length;
  const lines = text.split("\n");
  const each = caretParts(text, count, parts) ?? (count > 1 && lines.length === count ? lines : null);
  if (!each) return state.replaceSelection(text);
  let i = 0;
  return state.changeByRange((range) => {
    const insert = each[i++];
    return {
      changes: { from: range.from, to: range.to, insert },
      range: EditorSelection.cursor(range.from + insert.length),
    };
  });
}

/** Paste into an editor (Paste, Paste as Plain Text, Paste from History). */
export function pasteInto(view: EditorView, text: string, parts?: string[]): void {
  if (!text) return;
  view.dispatch(pasteSpec(view.state, text, parts), { scrollIntoView: true, userEvent: "input.paste" });
}

/**
 * A native paste (Ctrl+V) that needs Forge's caret parts: the clipboard holds the last copy
 * made with N carets and N carets are active, but the parts have line breaks of their own.
 * Everything else is left to CodeMirror.
 */
export function needsCaretPaste(state: EditorState, text: string): boolean {
  const count = state.selection.ranges.length;
  const normalized = normalize(text);
  return caretParts(normalized, count) != null && normalized.split("\n").length !== count;
}
