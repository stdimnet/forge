/**
 * Optimize Imports (Ctrl+Alt+O): in the editor the current file (PHP by Forge, JavaScript,
 * TypeScript and Vue by the TypeScript server), in the Project view the selected folders
 * and files after the "Optimize Imports" dialog. Rust computes the changes and the
 * notification; this module applies them.
 */
import type { EditorView } from "@codemirror/view";
import { importsApi } from "@/ipc/imports";
import { showBalloon } from "@/keymap";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { viewFor } from "@/features/lsp/navigation";
import { applyWorkspaceEdit } from "@/features/codeintel/refactoring";
import { editor } from "@/state/editor";
import { lspChanges, optimizesImports } from "./model";
import { askOptimizeImports, type OptimizeTarget } from "./OptimizeImportsDialog";
import { t as tr } from "@/i18n";

const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Optimize the imports of the file in `view`; the result is shown as a hint at the caret. */
export async function optimizeInEditor(path: string, view: EditorView): Promise<void> {
  const caret = view.state.selection.main.head;
  if (!optimizesImports(path)) {
    showHint(view, caret, "Optimize Imports is not available for this file type");
    return;
  }
  const before = view.state.doc;
  try {
    await syncNow(path);
    const answer = await importsApi.optimize(path, before.toString());
    if (view.state.doc !== before) return; // Edited meanwhile: the answer is stale.
    if (answer.edit) {
      view.dispatch({ changes: lspChanges(view, [answer.edit]), userEvent: "input.optimizeImports", scrollIntoView: false });
    }
    showHint(view, view.state.selection.main.head, answer.message);
  } catch (e) {
    showHint(view, caret, errorText(e));
  }
}

/** The texts of the open editors (they win over the files on disk). */
function openBuffers(): Record<string, string> {
  const out: Record<string, string> = {};
  for (const t of editor.tabs) {
    const v = viewFor(t.path);
    if (v) out[t.path] = v.state.doc.toString();
  }
  return out;
}

/** Project view: ask for the options, then optimize the imports of `targets`. */
export async function optimizePaths(targets: OptimizeTarget[]): Promise<void> {
  const options = await askOptimizeImports(targets);
  if (!options) return;
  try {
    const paths = targets.map((t) => t.path);
    const answer = await importsApi.optimizePaths({ paths, ...options, buffers: openBuffers() });
    if (answer.changed) await applyWorkspaceEdit(answer.edit, { label: "Optimize Imports" });
    showBalloon(answer.message);
  } catch (e) {
    showBalloon(tr("tools.imports.optimizeImportsFailed"), errorText(e), 6000);
  }
}
