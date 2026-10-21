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
export const countLabel = (n: number, what = "usage", many = `${what}s`) => `${n} ${n === 1 ? what : many}`;

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
  /** `usages`: Find Usages (grouped by usage type); `text`: Find in Files results. */
  kind: "usages" | "text";
  /** Tab title ("Usages of Order", "Occurrences of 'total'"). */
  title: string;
  /** Root node text ("Usages of Order in Project Files"). */
  header: string;
  answer: UsagesAnswer;
  /** Search again with the same options (Rerun). */
  rerun?: () => Promise<UsagesAnswer>;
  /** The selected usage (`path:line:column`), for Next/Previous Occurrence. */
  selected?: string;
}

export interface FindTabOptions {
  kind?: FindTab["kind"];
  title?: string;
  header?: string;
  rerun?: FindTab["rerun"];
}

const [findTabs, setFindTabs] = createSignal<FindTab[]>([]);
const [activeFindTab, setActiveFindTab] = createSignal<number | null>(null);
export { findTabs, activeFindTab, setActiveFindTab };

let nextTab = 1;

/** Pin results in the Find tool window (a new tab) and show it. */
export function openInFindWindow(answer: UsagesAnswer, opts: FindTabOptions = {}): void {
  const tab: FindTab = {
    id: nextTab++,
    kind: opts.kind ?? "usages",
    title: opts.title ?? `Usages of ${answer.title}`,
    header: opts.header ?? `Usages of ${answer.title} in Project Files`,
    answer,
    rerun: opts.rerun,
  };
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

/** Replace a tab's results (Rerun). */
export function updateFindTab(id: number, answer: UsagesAnswer): void {
  setFindTabs((tabs) => tabs.map((t) => (t.id === id ? { ...t, answer } : t)));
}

export const usageKey = (item: UsageItem) => `${item.path}:${item.row.line}:${item.row.column}`;

export function selectFindUsage(id: number, key: string | undefined): void {
  setFindTabs((tabs) => tabs.map((t) => (t.id === id ? { ...t, selected: key } : t)));
}

/**
 * The usage after (or before) `selected` in display order, wrapping around; the first (or
 * last) one without a selection.
 */
export function neighbourUsage(items: readonly UsageItem[], selected: string | undefined, delta: 1 | -1): UsageItem | undefined {
  if (!items.length) return undefined;
  const i = selected ? items.findIndex((u) => usageKey(u) === selected) : -1;
  if (i < 0) return delta > 0 ? items[0] : items[items.length - 1];
  return items[(i + delta + items.length) % items.length];
}

/** The tree a tab shows: by usage type (Find Usages with grouping on) or by file. */
export const tabTree = (tab: FindTab, byType: boolean): UsageTree => (tab.kind === "usages" && byType ? tab.answer.byType : tab.answer.byFile);

/** Next/Previous Occurrence (Ctrl+Alt+Down/Up) in the active tab of the Find tool window. */
export function gotoFindOccurrence(delta: 1 | -1, byType: boolean): boolean {
  const tab = findTabs().find((t) => t.id === activeFindTab());
  if (!tab) return false;
  const next = neighbourUsage(usageItems(tabTree(tab, byType)), tab.selected, delta);
  if (!next) return false;
  selectFindUsage(tab.id, usageKey(next));
  openUsage(next);
  return true;
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
