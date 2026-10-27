/**
 * Import paths through the project's aliases (`tsconfig` `paths`, Vite and Webpack
 * `resolve.alias`, read statically in Rust by `webforge-lang-web`): Ctrl+B / Ctrl+Click on
 * `'@/components/Nav.vue'` opens the file, and typing in an import string completes folders
 * and modules. The TypeScript language server only knows `tsconfig` paths, so this runs
 * first.
 */
import { EditorState } from "@codemirror/state";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { importsApi } from "@/ipc/webTooling";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { registerEditorExtension } from "@/editor/extensions";
import { openFileAt } from "@/editor/views";
import { isPluginActive } from "@/state/project";

const SCRIPT = /\.(m?[jt]sx?|cjs|vue|svelte|astro)$/i;
const active = () => isPluginActive("forge.web");

registerDeclarationProvider(async (path, view, pos) => {
  if (!SCRIPT.test(path) || !active()) return false;
  const line = view.state.doc.lineAt(pos);
  if (!/\bimport\b|\bfrom\b|\brequire\s*\(/.test(line.text)) return false;
  const target = await importsApi.goto(path, view.state.doc.toString(), pos).catch(() => null);
  if (!target) return false;
  await openFileAt(target.path, target.line + 1, target.column + 1);
  return true;
});

function importCompletion(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!active()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    // Only inside the string of an import / export … from / require().
    if (!/(?:\bfrom|\bimport|\brequire\s*\(|\bimport\s*\()\s*["'][^"']*$/.test(before)) return null;
    const answer = await importsApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return {
      from: answer.from,
      options: answer.items.map((i) => ({
        label: i.label,
        type: i.dir ? "namespace" : "variable",
        detail: i.dir ? "folder" : i.path,
        apply: i.dir ? `${i.label}/` : i.label,
        boost: i.dir ? 1 : 0,
      })),
      validFor: /^[\w@~#$.-]*$/,
    };
  };
}

registerEditorExtension(({ path }) => {
  if (!SCRIPT.test(path)) return null;
  const data = [{ autocomplete: importCompletion(path) }];
  return EditorState.languageData.of(() => data);
});
