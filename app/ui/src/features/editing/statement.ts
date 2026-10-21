/**
 * Complete Current Statement (`Ctrl+Shift+Enter`, `EditorCompleteStatement`): Rust
 * (`forge-editing`) works out the missing `;`, brackets and block; this applies the edit.
 */
import { EditorSelection } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { indentUnit } from "@codemirror/language";
import { insertNewlineAndIndent } from "@codemirror/commands";
import { editingApi, type StatementEdit } from "@/ipc/editing";

/** Apply a statement edit computed for `doc` to the view. */
export function applyStatementEdit(view: EditorView, edit: StatementEdit): void {
  view.dispatch({
    changes: edit.changes,
    selection: EditorSelection.cursor(edit.cursor),
    scrollIntoView: true,
    userEvent: "input.complete",
  });
}

export async function completeStatement(view: EditorView, fileType: string | undefined): Promise<void> {
  const { state } = view;
  const edit = fileType
    ? await editingApi
        .completeStatement({
          language: fileType,
          text: state.doc.toString(),
          offset: state.selection.main.head,
          indent: state.facet(indentUnit),
        })
        .catch(() => null)
    : null;
  // The document changed while Rust was working: do nothing rather than edit the wrong place.
  if (view.state.doc !== state.doc) return;
  if (edit) applyStatementEdit(view, edit);
  else insertNewlineAndIndent(view);
}
