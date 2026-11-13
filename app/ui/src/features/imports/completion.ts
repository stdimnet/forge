/**
 * Auto Import on completion in PHP: a completed class gets its `use` statement where the
 * code style puts it, or is written fully qualified when the Auto Import settings say so
 * (namespace / file scope, "Prefer FQN", PHPDoc, global classes). Rust decides
 * (`php_import_reference`); the language server's own `use` edit is not applied.
 */
import { syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import type { EditorCompletionItem } from "@/ipc/lsp";
import { importsApi } from "@/ipc/imports";
import { registerCompletionApplyHook } from "@/features/lsp/completion";
import { isPhpFile, offsetChanges } from "./model";

const CLASS_KINDS = new Set(["class", "interface", "enum", "struct"]);
const QUALIFIED = /^\\?[A-Za-z_\x80-￿][\w\x80-￿]*(\\[A-Za-z_\x80-￿][\w\x80-￿]*)*$/;

/** The fully qualified class a completion item inserts: from its `use` edit or its detail. */
export function completedClass(item: Pick<EditorCompletionItem, "kind" | "detail" | "label" | "additionalTextEdits">, inserted: string): string | null {
  if (!CLASS_KINDS.has(item.kind)) return null;
  for (const e of item.additionalTextEdits ?? []) {
    const m = /\buse\s+([\w\\\x80-￿]+)\s*;/.exec(e.newText);
    if (m) return m[1].replace(/^\\+/, "");
  }
  const detail = item.detail?.trim() ?? "";
  if (detail.includes("\\") && QUALIFIED.test(detail) && detail.split("\\").pop() === item.label) return detail.replace(/^\\+/, "");
  if (inserted.includes("\\") && QUALIFIED.test(inserted)) return inserted.replace(/^\\+/, "");
  return null;
}

/** Inside a comment (PHPDoc) at `pos`. */
function inComment(state: EditorState, pos: number): boolean {
  for (let n: ReturnType<ReturnType<typeof syntaxTree>["resolveInner"]> | null = syntaxTree(state).resolveInner(pos, -1); n; n = n.parent) {
    if (/comment/i.test(n.name)) return true;
  }
  return false;
}

registerCompletionApplyHook(({ path, view, item, from, to }) => {
  if (!isPhpFile(path)) return false;
  const doc = view.state.doc;
  const fqn = completedClass(item, doc.sliceString(from, to));
  if (!fqn) return false;
  const inDoc = inComment(view.state, from);
  void importsApi
    .phpReference({ content: doc.toString(), start: from, end: to, fqn, doc: inDoc, auto: true })
    .then((edits) => {
      if (view.state.doc !== doc || !edits.length) return;
      view.dispatch({ changes: offsetChanges(edits), userEvent: "input.complete" });
    })
    .catch(() => undefined);
  return true;
});
