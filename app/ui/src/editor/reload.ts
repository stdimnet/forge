/**
 * Replace the whole document with text that changed on disk (rollback, checkout, pull, an
 * agent's edit). Undo must not step back over such a reload: the old history would map onto
 * a different document and "undo" would delete or restore unrelated lines. So the reload is
 * not recorded and the history starts over.
 */
import { Compartment, Transaction } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { history } from "@codemirror/commands";

/** Holds the editor's undo history so a reload can reset it. */
export const historyCompartment = new Compartment();

export const undoHistory = () => historyCompartment.of(history());

export function replaceDocument(view: EditorView, text: string): void {
  if (text === view.state.doc.toString()) return;
  const head = Math.min(view.state.selection.main.head, text.length);
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
    selection: { anchor: head },
    annotations: Transaction.addToHistory.of(false),
  });
  // Dropping and re-adding the history field discards everything recorded before.
  view.dispatch({ effects: historyCompartment.reconfigure([]) });
  view.dispatch({ effects: historyCompartment.reconfigure(history()) });
}
