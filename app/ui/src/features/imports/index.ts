/**
 * Optimize Imports and Auto Import, like PHPStorm:
 *
 * - Code | Optimize Imports (Ctrl+Alt+O, `OptimizeImports`): the current file in the editor;
 *   the selected folders and files in the Project view (with the "Optimize Imports" dialog).
 *   Also in the editor's and the Project view's context menus.
 * - Settings | Editor | General | Auto Import (schema page `editor.general.auto-import`).
 * - The import popup, Auto Import on the fly and on completion (editor extensions).
 * - "Import class" with several candidates: the "Class to Import" chooser.
 *
 * The import logic is Rust (`phpforge_lang_php::imports`, the TypeScript server through
 * `app/src-tauri/src/commands/imports.rs`); this feature renders and applies.
 */
import { render } from "solid-js/web";
import { registerEditorExtension } from "@/editor/extensions";
import type { ImportChoice } from "@/ipc/imports";
import { workspaceApi, type FileEntry } from "@/ipc/workspace";
import { registerCommand } from "@/state/commands";
import { isDialogWindow } from "@/shell/window/mode";
import { activeView } from "@/features/lsp/navigation";
import { registerContextActionHandler } from "@/features/codeintel/intentions";
import { item, registerContextMenuProvider } from "@/features/contextMenu/state";
import { projectTree } from "@/features/project/ProjectView";
import { chooseClassToImport } from "./chooser";
import { importHint } from "./hint";
import { importsOnTheFly } from "./onTheFly";
import { OptimizeImportsDialogHost, type OptimizeTarget } from "./OptimizeImportsDialog";
import { optimizeInEditor, optimizePaths } from "./optimize";
import { optimizesImports } from "./model";
import "./completion";
import "./imports.css";

const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");

/** The selected Project view entries with their kind (folder or file). */
async function selectedTargets(): Promise<OptimizeTarget[]> {
  const paths = projectTree.selectedPaths();
  const out: OptimizeTarget[] = [];
  for (const path of paths) {
    const entry = await workspaceApi
      .listDir(parentOf(path))
      .then((entries) => entries.find((e) => e.path === path))
      .catch(() => undefined);
    if (entry) out.push({ path, dir: entry.kind === "dir" });
  }
  return out;
}

registerCommand({
  id: "OptimizeImports",
  title: "Optimize Imports",
  category: "Code",
  run: async () => {
    if (document.activeElement?.closest(".project-tree")) {
      const targets = await selectedTargets();
      if (targets.length) await optimizePaths(targets);
      return;
    }
    const a = activeView();
    if (a) await optimizeInEditor(a.path, a.view);
  },
});

// Project view: next to Reload from Disk, for folders and the files it handles.
registerContextMenuProvider<FileEntry | null>("project", (entry) => {
  if (entry && entry.kind !== "dir" && !optimizesImports(entry.path)) return [];
  const targets = (): OptimizeTarget[] => {
    if (!entry) return [{ path: "", dir: true }];
    const selected = projectTree.selectedPaths();
    if (selected.length > 1 && selected.includes(entry.path)) {
      return selected.map((p) => ({ path: p, dir: p === entry.path ? entry.kind === "dir" : !optimizesImports(p) }));
    }
    return [{ path: entry.path, dir: entry.kind === "dir" }];
  };
  return [item("Optimize Imports…", () => void optimizePaths(targets()), "OptimizeImports")];
});

registerContextActionHandler("PhpImportClass", (data, ctx) => {
  chooseClassToImport(ctx.path, ctx.view, data as ImportChoice);
});

registerEditorExtension(({ path }) => [importHint(path), importsOnTheFly(path)]);

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "optimize-imports-overlay";
  document.body.appendChild(host);
  render(OptimizeImportsDialogHost, host);
}
