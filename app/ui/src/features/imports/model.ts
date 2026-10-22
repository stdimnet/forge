/**
 * Shared bits of Optimize Imports and Auto Import: which files they handle and how their
 * answers become editor changes.
 */
import type { EditorView } from "@codemirror/view";
import type { OffsetEdit } from "@/ipc/imports";
import type { TextEditLsp, WorkspaceEdit } from "@/ipc/codeintel";
import { fromLsp } from "@/features/lsp/util";

export const isPhpFile = (path: string) => /\.(php|phtml|inc)$/i.test(path) && !/\.blade\.php$/i.test(path);
export const isWebScript = (path: string) => /\.(vue|jsx|tsx|js|ts|mjs|cjs|mts|cts)$/i.test(path) && !/\.d\.ts$/i.test(path);
export const optimizesImports = (path: string) => isPhpFile(path) || isWebScript(path);

/** CodeMirror changes of offset edits (UTF-16 offsets, as CodeMirror counts). */
export function offsetChanges(edits: OffsetEdit[]) {
  return edits.map((e) => ({ from: e.start, to: e.end, insert: e.newText }));
}

/** CodeMirror changes of LSP text edits on the view's document. */
export function lspChanges(view: EditorView, edits: TextEditLsp[]) {
  const doc = view.state.doc;
  return edits.map((e) => ({ from: fromLsp(doc, e.range.start), to: fromLsp(doc, e.range.end), insert: e.newText }));
}

/** The text edits of `path` in a workspace edit. */
export function editsOf(edit: WorkspaceEdit, path: string): TextEditLsp[] {
  return edit.changes.flatMap((c) => (c.kind === "edit" && c.path === path ? c.edits : []));
}

/** `App\Models\User` -> `User` and `App\Models`. */
export function splitFqn(fqn: string): { name: string; namespace: string } {
  const clean = fqn.replace(/^\\+/, "");
  const at = clean.lastIndexOf("\\");
  return at < 0 ? { name: clean, namespace: "" } : { name: clean.slice(at + 1), namespace: clean.slice(0, at) };
}

/** The import popup's text: the class, or "Multiple choices…" when several classes match. */
export function hintText(candidates: string[], shortcut: string): string {
  const what = candidates.length === 1 ? candidates[0] : "Multiple choices…";
  return shortcut ? `? ${what} ${shortcut}` : `? ${what}`;
}

/** "Add import from "./x"" (TypeScript) -> the module, or null for other fixes. */
export function webImportModule(title: string): string | null {
  const m = /^(?:Add|Update) import from "([^"]+)"/.exec(title);
  return m ? m[1] : null;
}
