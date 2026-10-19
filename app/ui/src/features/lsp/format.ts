/** Reformat Code (Ctrl+Alt+L) through the language server's document formatting. */
import type { ChangeSpec, Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { lspApi, type TextEdit } from "@/ipc/lsp";
import { syncNow } from "./sync";
import { fromLsp } from "./util";

/** Apply LSP text edits (positions relative to the current document) in one transaction. */
export function applyTextEdits(view: EditorView, edits: TextEdit[], userEvent = "format"): void {
  if (!edits.length) return;
  const doc = view.state.doc;
  const changes: ChangeSpec[] = edits
    .map((e) => ({ from: fromLsp(doc, e.range.start), to: fromLsp(doc, e.range.end), insert: e.newText }))
    .sort((a, b) => a.from - b.from);
  view.dispatch({ changes, userEvent, scrollIntoView: false });
}

export async function reformat(path: string, view: EditorView): Promise<void> {
  await syncNow(path);
  const edits = await lspApi.format(path).catch(() => [] as TextEdit[]);
  applyTextEdits(view, edits);
}

export function lspFormat(path: string): Extension {
  return keymap.of([{ key: "Mod-Alt-l", run: (view) => (void reformat(path, view), true) }]);
}
