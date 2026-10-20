/**
 * Split editors show one document in several CodeMirror views (one per editor group). Each
 * view forwards its document changes to the other views of the same file as they happen, so
 * all of them hold the same text, keep their own caret and scroll position, and map
 * decorations (bookmarks, markers) through the edit instead of reloading.
 *
 * Forwarded changes are not recorded in the receiving view's undo history: undo in a view
 * undoes what was typed there, mapped through the edits made in the other views.
 */
import { Annotation, Transaction, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

const synced = Annotation.define<boolean>();

const peers = new Map<string, Set<EditorView>>();

/** Keep this view's document in sync with the other views of `path`. */
export function docSync(path: string): Extension {
  return [
    EditorView.updateListener.of((u) => {
      if (!u.docChanged) return;
      for (const tr of u.transactions) {
        if (!tr.docChanged || tr.annotation(synced)) continue;
        for (const other of peers.get(path) ?? []) {
          if (other === u.view) continue;
          other.dispatch({ changes: tr.changes, annotations: [synced.of(true), Transaction.addToHistory.of(false)] });
        }
      }
    }),
  ];
}

/** Register a mounted view; returns the cleanup. */
export function joinDocSync(path: string, view: EditorView): () => void {
  const set = peers.get(path) ?? new Set<EditorView>();
  set.add(view);
  peers.set(path, set);
  return () => {
    set.delete(view);
    if (!set.size) peers.delete(path);
  };
}

/** Whether a transaction came from another view of the same document. */
export const isSyncedTransaction = (tr: Transaction): boolean => !!tr.annotation(synced);
