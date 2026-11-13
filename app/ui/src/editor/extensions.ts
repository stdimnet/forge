/**
 * Editor extension contribution point. Language packs and features (LSP hover, blame,
 * inspections) register CodeMirror extensions here; they are applied per file.
 */
import type { Extension } from "@codemirror/state";

export type EditorExtensionFactory = (ctx: { path: string }) => Extension | null;

const factories: EditorExtensionFactory[] = [];

export function registerEditorExtension(factory: EditorExtensionFactory): void {
  factories.push(factory);
}

export function extensionsFor(path: string): Extension[] {
  return factories.map((f) => f({ path })).filter((e): e is Extension => e != null);
}
