/**
 * Replace the whole document with text that changed on disk (rollback, checkout, pull, an
 * agent's edit). Undo must not step back over such a reload: the old history would map onto
 * a different document and "undo" would delete or restore unrelated lines. So the reload is
 * not recorded and the history starts over.
 */
import { Compartment, Transaction } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { history, redo, undo } from "@codemirror/commands";

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

/**
 * Undo and Redo of the Edit menu. Views can bring their own history (shared Code With Me
 * documents undo only the participant's own edits); `registerHistory` installs it.
 */
export interface ViewHistory {
  undo: (view: EditorView) => boolean;
  redo: (view: EditorView) => boolean;
}

const histories: ((view: EditorView) => ViewHistory | undefined)[] = [];

export function registerHistory(lookup: (view: EditorView) => ViewHistory | undefined): void {
  histories.push(lookup);
}

const historyOf = (view: EditorView): ViewHistory =>
  histories.map((h) => h(view)).find((h) => h) ?? { undo: (v) => undo(v), redo: (v) => redo(v) };

export const undoIn = (view: EditorView): boolean => historyOf(view).undo(view);
export const redoIn = (view: EditorView): boolean => historyOf(view).redo(view);
