/**
 * Project structure, like PHPStorm: the Project view's "Mark Directory as" (Sources Root and
 * Test Sources Root of PHP projects, Resource Root, Test Resources Root, Excluded / Cancel
 * Exclusion), resource roots in the tree, and what happens when the excluded folders change
 * (the backend already skips them; the tree is read again and the PHP index rebuilt).
 */
import { createEffect, createRoot, on } from "solid-js";
import type { FileEntry } from "@/ipc/workspace";
import { projectStructureApi, type MarkAction } from "@/ipc/projectStructure";
import { workspace } from "@/state/workspace";
import { isPluginActive } from "@/state/project";
import { isDialogWindow } from "@/shell/window/mode";
import { item, registerContextMenuProvider, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { refreshTree, registerRowDecorator } from "@/features/project/ProjectView";
import { sourceRootItems } from "@/features/php/sourceRoots";
import { indexWorkspace } from "@/features/php/status";
import { folderState, markDirectory, structureConfig } from "./model";
import "./FileNestingDialog";
import "./projectStructure.css";

/** The language-agnostic entries of "Mark Directory as" for a folder. */
export function markItems(entry: FileEntry): MenuEntry[] {
  const state = folderState(entry.path, true);
  const run = (action: MarkAction) => () => void markDirectory(entry.path, action);
  const items: MenuEntry[] = [];
  if (state.reason === "parent") return items;
  if (state.kind !== "resource") items.push(item("Resource Root", run("resource")));
  if (state.kind !== "testResource") items.push(item("Test Resources Root", run("testResource")));
  if (state.kind !== "excluded") items.push(item("Excluded", run("excluded")));
  if (state.kind === "resource") items.push(item("Unmark as Resource Root", run("unmark")));
  if (state.kind === "testResource") items.push(item("Unmark as Test Resources Root", run("unmark")));
  if (state.kind === "excluded") items.push(item("Cancel Exclusion", run("cancelExclusion")));
  return items;
}

// Project view: Mark Directory as (every folder; PHP projects add their source roots first).
registerContextMenuProvider<FileEntry | null>("project.compare", (entry) => {
  if (!entry || entry.kind !== "dir") return [];
  const items = [...sourceRootItems(entry), ...markItems(entry)];
  return items.length ? [submenu("Mark Directory as", items)] : [];
});

// Resource roots get their own folder colors (excluded rows are flagged by Rust).
registerRowDecorator((path, dir) => {
  if (!dir) return undefined;
  const marked = structureConfig().folders.find((f) => f.path === path);
  if (marked?.kind === "resource") return { class: "resource-root" };
  if (marked?.kind === "testResource") return { class: "test-resource-root" };
  return undefined;
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  // The backend skips the new excluded folders already: read the tree again and rebuild
  // the PHP index without (or with) them.
  void projectStructureApi.onChanged(() => {
    refreshTree();
    if (workspace() && isPluginActive("forge.php")) void indexWorkspace();
  });
  // Re-read at once when the settings change (browser mode has no backend event).
  createRoot(() =>
    createEffect(
      on(
        () => JSON.stringify(structureConfig()),
        () => refreshTree(),
        { defer: true },
      ),
    ),
  );
}
