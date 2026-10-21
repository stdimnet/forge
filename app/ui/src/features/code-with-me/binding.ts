/**
 * The CodeMirror side of shared documents: every editor gets an (empty) compartment; while
 * its tab is shared, the compartment holds `y-codemirror.next` (sync, remote carets and
 * selections, the participant's own undo) and the editor's own history is switched off.
 */
import { Compartment, EditorState, Prec, Transaction } from "@codemirror/state";
import { EditorView, keymap, ViewPlugin } from "@codemirror/view";
import { history } from "@codemirror/commands";
import { yCollab, yUndoManagerKeymap } from "y-codemirror.next";
import { registerEditorExtension } from "@/editor/extensions";
import { historyCompartment, registerHistory } from "@/editor/reload";
import { sharedDoc, type SharedDoc } from "./docs";

const collab = new Compartment();
const views = new Map<string, EditorView>();
const bound = new WeakMap<EditorView, SharedDoc>();

/** Carets 2 px wide with the name above them, in the theme's participant colors. */
const collabTheme = EditorView.theme({
  ".cm-ySelectionCaret": { borderLeftWidth: "1px", borderRightWidth: "1px" },
  ".cm-ySelectionCaretDot": { display: "none" },
  ".cm-ySelectionInfo": {
    opacity: 1,
    top: "-1.35em",
    padding: "0 4px",
    borderRadius: "3px 3px 3px 0",
    fontFamily: "var(--font-ui)",
    fontSize: "11px",
    lineHeight: "1.35em",
    color: "var(--collab-label-fg)",
  },
});

function bind(view: EditorView, doc: SharedDoc): void {
  const text = doc.ytext.toString();
  if (view.state.doc.toString() !== text) {
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: text },
      annotations: Transaction.addToHistory.of(false),
    });
  }
  view.dispatch({
    effects: [
      historyCompartment.reconfigure([]),
      collab.reconfigure([
        Prec.high(keymap.of(yUndoManagerKeymap)),
        yCollab(doc.ytext, doc.awareness, { undoManager: doc.undo }),
        EditorState.readOnly.of(doc.readOnly),
        collabTheme,
      ]),
    ],
  });
  bound.set(view, doc);
}

function unbind(view: EditorView): void {
  // The history starts fresh: the shared undo stack does not map onto the local one.
  view.dispatch({ effects: [collab.reconfigure([]), historyCompartment.reconfigure(history())] });
  bound.delete(view);
}

/** Bind or unbind the editor of `path` to match whether its tab is shared. */
export function syncBinding(path: string): void {
  const view = views.get(path);
  if (!view) return;
  const doc = sharedDoc(path);
  const current = bound.get(view);
  if (doc === current) return;
  if (current) unbind(view);
  if (doc) bind(view, doc);
}

export function syncAllBindings(): void {
  for (const path of views.keys()) syncBinding(path);
}

export const boundDoc = (view: EditorView): SharedDoc | undefined => bound.get(view);

registerEditorExtension(({ path }) => [
  collab.of([]),
  ViewPlugin.define((view) => {
    views.set(path, view);
    // Views cannot be reconfigured while they are being built.
    queueMicrotask(() => syncBinding(path));
    return {
      destroy() {
        if (views.get(path) === view) views.delete(path);
        bound.delete(view);
      },
    };
  }),
]);

// Edit | Undo / Redo in a shared editor run the participant's own undo manager.
registerHistory((view) => {
  const doc = bound.get(view);
  if (!doc) return undefined;
  return { undo: () => (doc.undo.undo(), true), redo: () => (doc.undo.redo(), true) };
});
