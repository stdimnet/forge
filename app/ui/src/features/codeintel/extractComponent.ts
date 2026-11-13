/**
 * Extract Component (WebForge) for Vue single-file components and React JSX, like
 * WebStorm: the selected template elements or JSX become a new component file with props for
 * the names they use, and the parent renders and imports it. Rust analyses the selection and
 * computes the new file and the edits (`refactor_extract_component*`); this module shows the
 * dialog and applies the answer as one undoable refactoring.
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { isLibraryPath } from "@/ipc/codeintel";
import { extractComponentApi, type ExtractComponentInfo, type RefactorAnswer } from "@/ipc/refactor";
import { openFile } from "@/state/editor";
import { showHint } from "@/features/lsp/hint";
import { toLsp } from "@/features/lsp/util";
import { applyAnswer } from "./extract";
import { renameSessionOf } from "./rename";
import { t } from "@/i18n";

/** WebForge's Extract Component action. */
export const EXTRACT_COMPONENT = "WebForge.ExtractComponent";

const message = (e: unknown) => String(e instanceof Error ? e.message : e);
const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");

export interface ExtractComponentRequest {
  info: ExtractComponentInfo;
  directory: string;
  done: (value: { name: string; directory: string; preview: boolean } | null) => void;
}

const [extractComponentDialog, setExtractComponentDialog] = createSignal<ExtractComponentRequest | null>(null);
export { extractComponentDialog };

/** Files Extract Component works on. */
export const isComponentFile = (path: string) => /\.(vue|jsx|tsx|js|ts|mjs)$/i.test(path) && !/\.d\.ts$/i.test(path);

/** The dialog's validation message for a component name, or `null`. */
export function validateComponentName(name: string): string | null {
  const n = name.trim();
  if (!n) return "Specify the component name";
  return /^[A-Z][A-Za-z0-9_]*$/.test(n) ? null : `'${n}' is not a valid component name (use PascalCase)`;
}

/** The new file's path as the dialog shows it. */
export function componentPath(directory: string, name: string, extension: string): string {
  const dir = directory.trim().replace(/^\/+|\/+$/g, "");
  return `${dir ? `${dir}/` : ""}${name.trim() || "…"}.${extension}`;
}

/** Extract Component for the selection of the editor. */
export async function extractComponentAt(path: string, view: EditorView): Promise<void> {
  const sel = view.state.selection.main;
  if (isLibraryPath(path) || view.state.readOnly) {
    showHint(view, sel.head, "Cannot perform refactoring in a read-only file");
    return;
  }
  if (renameSessionOf(view)) return;
  if (!isComponentFile(path)) {
    showHint(view, sel.head, "Extract Component is available in Vue and React files");
    return;
  }
  if (sel.empty) {
    showHint(view, sel.head, "Select the template elements or JSX to extract");
    return;
  }
  const doc = view.state.doc;
  const range = { start: toLsp(doc, sel.from), end: toLsp(doc, sel.to) };
  const content = doc.toString();
  let info: ExtractComponentInfo;
  try {
    info = await extractComponentApi.prepare(path, range, content);
  } catch (e) {
    showHint(view, sel.head, message(e));
    return;
  }
  const choice = await new Promise<{ name: string; directory: string; preview: boolean } | null>((resolve) =>
    setExtractComponentDialog({
      info,
      directory: dirOf(path),
      done: (v) => {
        setExtractComponentDialog(null);
        resolve(v);
      },
    }),
  );
  view.focus();
  if (!choice) return;
  let answer: RefactorAnswer;
  try {
    answer = await extractComponentApi.extract(path, range, content, choice.name, choice.directory);
  } catch (e) {
    showHint(view, sel.head, message(e));
    return;
  }
  if (await applyAnswer(t("codeintel.extractComponent.extractComponentName", { name: choice.name }), answer, { preview: choice.preview })) {
    const created = answer.edit.changes.find((c) => c.kind === "create");
    if (created && created.kind === "create") await openFile(created.path).catch(() => undefined);
  }
}
