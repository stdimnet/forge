/**
 * Find Usages (Alt+F7) and Show Usages (Ctrl+Alt+F7, Ctrl+Click on a declaration): the
 * usages come from Rust (`code_find_usages`: language servers, else the PHP index), grouped
 * by usage type and file there; the popup and the Find tool window only render them.
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { editor, isDirty } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { openFileAt } from "@/editor/views";
import { codeApi, isLibraryPath, type UsageRow, type UsagesAnswer, type UsageTree } from "@/ipc/codeintel";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";

export interface UsagesPopupState {
  answer: UsagesAnswer;
  x: number;
  y: number;
}

const [usagesPopup, setUsagesPopup] = createSignal<UsagesPopupState | null>(null);
export { usagesPopup };
export const closeUsagesPopup = () => setUsagesPopup(null);

/** One usage with its file, in display order. */
export interface UsageItem {
  path: string;
  row: UsageRow;
}

/** Usages of a tree in order (groups, files, lines). */
export function usageItems(tree: UsageTree): UsageItem[] {
  return tree.groups.flatMap((g) => g.files.flatMap((f) => f.usages.map((row) => ({ path: f.path, row }))));
}

/** "12 usages", "1 usage". */
export const countLabel = (n: number, what = "usage") => `${n} ${what}${n === 1 ? "" : "s"}`;

export function openUsage(item: UsageItem): void {
  void openFileAt(item.path, item.row.line + 1, item.row.column + 1);
}

/** Unsaved editor contents, so usages match what is on screen. */
function dirtyBuffers(): Record<string, string> {
  return Object.fromEntries(editor.tabs.filter((t) => isDirty(t) && !isLibraryPath(t.path)).map((t) => [t.path, t.content]));
}

async function queryUsages(path: string, view: EditorView, pos: number): Promise<UsagesAnswer | null> {
  if (isLibraryPath(path)) {
    showHint(view, pos, "Usages are searched in project files only");
    return null;
  }
  await syncNow(path);
  try {
    return await codeApi.findUsages(path, toLsp(view.state.doc, pos), view.state.doc.toString(), dirtyBuffers());
  } catch (e) {
    showHint(view, pos, String(e instanceof Error ? e.message : e));
    return null;
  }
}

/**
 * Show Usages popup below `pos`. With `jumpIfSingle` (Go to Declaration on a declaration)
 * a single usage opens directly, like PHPStorm.
 */
export async function showUsagesAt(path: string, view: EditorView, pos: number, opts: { jumpIfSingle?: boolean } = {}): Promise<void> {
  const answer = await queryUsages(path, view, pos);
  if (!answer) return;
  const items = usageItems(answer.byFile);
  if (!items.length) {
    showHint(view, pos, `No usages of ${answer.title || "the symbol"} found in Project Files`);
    return;
  }
  if (opts.jumpIfSingle && items.length === 1) {
    openUsage(items[0]);
    return;
  }
  const coords = view.coordsAtPos(pos);
  setUsagesPopup({ answer, x: coords?.left ?? 200, y: (coords?.bottom ?? 200) + 4 });
}

// ---- Find tool window ----

export interface FindTab {
  id: number;
  answer: UsagesAnswer;
}

const [findTabs, setFindTabs] = createSignal<FindTab[]>([]);
const [activeFindTab, setActiveFindTab] = createSignal<number | null>(null);
export { findTabs, activeFindTab, setActiveFindTab };

let nextTab = 1;

/** Pin usages in the Find tool window (a new tab) and show it. */
export function openInFindWindow(answer: UsagesAnswer): void {
  const tab = { id: nextTab++, answer };
  setFindTabs((tabs) => [...tabs.slice(-9), tab]);
  setActiveFindTab(tab.id);
  closeUsagesPopup();
  showToolWindow("bottom", "find");
}

export function closeFindTab(id: number): void {
  const rest = findTabs().filter((t) => t.id !== id);
  setFindTabs(rest);
  if (activeFindTab() === id) setActiveFindTab(rest.at(-1)?.id ?? null);
}

/** Find Usages (Alt+F7): the usages in the Find tool window. */
export async function findUsagesAt(path: string, view: EditorView, pos: number): Promise<void> {
  const answer = await queryUsages(path, view, pos);
  if (!answer) return;
  if (!answer.byFile.total) {
    showHint(view, pos, `No usages of ${answer.title || "the symbol"} found in Project Files`);
    return;
  }
  openInFindWindow(answer);
}
