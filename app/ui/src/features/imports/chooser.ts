/**
 * "Import class" with several classes of the same name: PHPStorm's "Class to Import"
 * chooser (the shared Choose Declaration popup), then the import as the code style says
 * (`php_import_reference`: a `use` statement in its group, or the fully qualified name).
 */
import type { EditorView } from "@codemirror/view";
import { importsApi, type ImportChoice } from "@/ipc/imports";
import { showHint } from "@/features/lsp/hint";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { offsetChanges, splitFqn } from "./model";

/** Import `fqn` for the name `choice` points at. */
export async function importClass(view: EditorView, choice: ImportChoice, fqn: string): Promise<void> {
  const doc = view.state.doc;
  try {
    const edits = await importsApi.phpReference({ content: doc.toString(), start: choice.start, end: choice.end, fqn, doc: choice.doc });
    if (view.state.doc !== doc || !edits.length) return;
    view.dispatch({ changes: offsetChanges(edits), userEvent: "input.import" });
    view.focus();
  } catch (e) {
    showHint(view, view.state.selection.main.head, String(e instanceof Error ? e.message : e));
  }
}

/** Show the "Class to Import" chooser below the name. */
export function chooseClassToImport(_path: string, view: EditorView, choice: ImportChoice): void {
  const targets = choice.candidates.map((fqn) => {
    const { name, namespace } = splitFqn(fqn);
    return { path: `${name}.php`, line: 0, name, container: namespace || null, wholeFile: true, fqn };
  });
  chooseDeclarationAt(view, choice.start, targets, {
    title: "Class to Import",
    open: (t) => {
      const fqn = (t as (typeof targets)[number]).fqn;
      void importClass(view, choice, fqn);
    },
  });
}
