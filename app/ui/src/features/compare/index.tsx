/**
 * Compare Files, Compare Directories and Compare with Clipboard (PHPStorm's View | Compare
 * With… and Compare with Clipboard). Ctrl+D (`CompareTwoFiles`) compares two files or
 * folders selected in the Project view, or asks for the second one when one is selected;
 * other views register what Ctrl+D does for them (`registerCompareHandler`).
 */
import { Show } from "solid-js";
import { render } from "solid-js/web";
import { Columns2 } from "lucide-solid";
import { registerCommand } from "@/state/commands";
import { activeTab, openCustomTab } from "@/state/editor";
import { actionById, showBalloon } from "@/keymap";
import { item, registerContextMenuProvider, type MenuEntry } from "@/features/contextMenu/state";
import { projectTree } from "@/features/project/ProjectView";
import { activeView } from "@/features/navigation/editorCommands";
import { lastCopy } from "@/features/clipboard/clipboard";
import { setDiffRequest } from "@/features/git/overlays";
import type { FileEntry } from "@/ipc/workspace";
import { compareFiles, compareHandlerForFocus, registerCompareHandler, selectPathRequest, setSelectPathRequest } from "./state";
import { compareFocusedDirDiffRow, DirDiffView } from "./DirDiffView";
import { SelectPathDialog } from "./SelectPathDialog";
import "./compare.css";

const nameOf = (path: string) => path.split("/").pop() || path || "/";

/** Compare Directories: the folder diff of two project folders in an editor tab. */
export function openDirDiff(left: string, right: string): void {
  openCustomTab({
    id: `forge:compare-dirs:${left}\u0000${right}`,
    title: `${nameOf(left)} vs ${nameOf(right)}`,
    tooltip: `${left || "/"} vs ${right || "/"}`,
    icon: Columns2,
    component: () => <DirDiffView left={left} right={right} />,
  });
}

interface Picked {
  path: string;
  dir: boolean;
}

/** The Project view selection in display order, with which rows are folders. */
function projectSelection(): Picked[] {
  const paths = projectTree.selectedPaths();
  const rows = [...document.querySelectorAll<HTMLElement>(".project-tree .tree-row[data-path]")];
  const order = (p: string) => rows.findIndex((r) => r.dataset.path === p);
  return paths
    .map((path) => ({ path, dir: rows.find((r) => r.dataset.path === path)?.dataset.dir !== undefined }))
    .sort((a, b) => order(a.path) - order(b.path));
}

/** Compare the two selected entries, or ask for the second one (Select Path). */
export function compareSelection(selection: Picked[]): void {
  if (selection.length === 2) {
    const [a, b] = selection;
    if (a.dir !== b.dir) return void showBalloon("Cannot compare a file with a folder", "Select two files or two folders.");
    return a.dir ? openDirDiff(a.path, b.path) : void compareFiles(a.path, b.path);
  }
  if (selection.length !== 1) return;
  const [a] = selection;
  setSelectPathRequest({
    title: a.dir ? "Select Directory to Compare With" : "Select File to Compare With",
    description: `Compare ${a.path || "the project root"} with:`,
    dirs: a.dir,
    initial: a.path,
    onOk: (path) => (a.dir ? openDirDiff(a.path, path) : compareFiles(a.path, path)),
  });
}

/** Ctrl+D: what the focused view registered, else the Project view selection. */
function compareTwoFiles(): void | Promise<void> {
  const handler = compareHandlerForFocus();
  if (handler) return handler.run();
  const focus = document.activeElement;
  if (focus?.closest(".project-tree")) compareSelection(projectSelection());
}

/** Compare with Clipboard: the clipboard against the selection, or the whole file. */
export async function compareWithClipboard(): Promise<void> {
  const view = activeView();
  const tab = activeTab();
  if (!view || !tab) return;
  let clip: string | null;
  try {
    clip = await navigator.clipboard.readText();
  } catch {
    // The system clipboard may be off limits (browser permissions); Forge's last copy is the same text.
    clip = lastCopy()?.text ?? null;
  }
  if (clip === null) return void showBalloon("Clipboard is not available", "Copy some text first.");
  const sel = view.state.selection.main;
  const name = nameOf(tab.path);
  const whole = sel.empty;
  setDiffRequest({
    path: tab.path,
    left: { title: "Clipboard", content: clip },
    right: whole
      ? { title: name, content: view.state.doc.toString() }
      : { title: `Selection from ${name}`, content: view.state.sliceDoc(sel.from, sel.to) },
    // Only a whole project file is written back after editing in the viewer.
    editable: whole && !tab.view && !tab.path.includes("://"),
  });
}

registerCommand({ id: "CompareTwoFiles", title: actionById("CompareTwoFiles")?.title ?? "Compare Files", category: "View", run: compareTwoFiles });
registerCommand({
  id: "CompareClipboardWithSelection",
  title: actionById("CompareClipboardWithSelection")?.title ?? "Compare with Clipboard",
  category: "View",
  run: compareWithClipboard,
});
registerCompareHandler(".dir-diff", compareFocusedDirDiffRow);

// Project view: Compare With… for one entry, Compare Files / Compare Directories for two.
registerContextMenuProvider<FileEntry | null>("project.compare", (entry) => {
  const selection = projectSelection();
  const picked = selection.length ? selection : [{ path: entry?.path ?? "", dir: !entry || entry.kind === "dir" }];
  let label = "Compare With…";
  if (picked.length === 2) label = picked[0].dir && picked[1].dir ? "Compare Directories" : "Compare Files";
  const enabled = picked.length === 1 || (picked.length === 2 && picked[0].dir === picked[1].dir);
  const entries: MenuEntry[] = [item(label, () => compareSelection(picked), "CompareTwoFiles", { disabled: !enabled })];
  return entries;
});

if (typeof window !== "undefined") {
  const host = document.createElement("div");
  host.id = "compare-overlays";
  document.body.append(host);
  render(() => <Show when={selectPathRequest()}>{(req) => <SelectPathDialog req={req()} />}</Show>, host);
}
