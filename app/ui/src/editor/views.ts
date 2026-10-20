/**
 * Live CodeMirror views by file path, so features outside the editor (AI chat, ...) can
 * read the selection, insert text at the caret of the active editor, or open a file at a
 * position (`openFileAt`).
 */
import type { Text } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { editor, openFile } from "@/state/editor";
import { flashLineAt } from "./flash";

const views = new Map<string, EditorView>();

export function registerView(path: string, view: EditorView): () => void {
  views.set(path, view);
  return () => {
    if (views.get(path) === view) views.delete(path);
  };
}

export const activeView = (): EditorView | undefined => (editor.active ? views.get(editor.active) : undefined);

export interface EditorSelection {
  path: string;
  startLine: number;
  endLine: number;
  text: string;
}

/** Non-empty selection of the active editor, if any. */
export function activeSelection(): EditorSelection | undefined {
  const view = activeView();
  if (!view || !editor.active) return undefined;
  const { from, to } = view.state.selection.main;
  if (from === to) return undefined;
  return {
    path: editor.active,
    startLine: view.state.doc.lineAt(from).number,
    endLine: view.state.doc.lineAt(to).number,
    text: view.state.sliceDoc(from, to),
  };
}

/** Replace the selection of the active editor with `text`. Returns false without an editor. */
export function insertAtCaret(text: string): boolean {
  const view = activeView();
  if (!view) return false;
  view.dispatch(view.state.replaceSelection(text));
  view.focus();
  return true;
}

/** Document offset of a 1-based line and column, clamped to the document. */
export function offsetAt(doc: Text, line: number, column = 1): number {
  const l = doc.line(Math.min(Math.max(1, Math.floor(line) || 1), doc.lines));
  return l.from + Math.min(Math.max(0, (Math.floor(column) || 1) - 1), l.length);
}

const nextFrame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

/** The view of `path` once it is mounted and visible (new tabs need a few frames). */
async function visibleView(path: string): Promise<EditorView | undefined> {
  for (let i = 0; i < 60; i++) {
    const view = views.get(path);
    if (view?.dom.isConnected && view.dom.offsetParent !== null) return view;
    await nextFrame();
  }
  return views.get(path);
}

/**
 * Open a workspace-relative file, put the caret at the 1-based `line`/`column`, scroll it
 * to the middle and (by default) flash the line, like PHPStorm does for navigation from
 * outside the IDE.
 */
export async function openFileAt(
  path: string,
  line?: number | null,
  column?: number | null,
  opts: { flash?: boolean; focus?: boolean } = {},
): Promise<boolean> {
  await openFile(path);
  const view = await visibleView(path);
  if (!view) return false;
  const pos = line ? offsetAt(view.state.doc, line, column ?? 1) : view.state.selection.main.head;
  view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
  if (opts.focus !== false) view.focus();
  if (line && opts.flash !== false) flashLineAt(view, pos);
  return true;
}
