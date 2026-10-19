/**
 * Live CodeMirror views by file path, so features outside the editor (AI chat, ...) can
 * read the selection or insert text at the caret of the active editor.
 */
import type { EditorView } from "@codemirror/view";
import { editor } from "@/state/editor";

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
