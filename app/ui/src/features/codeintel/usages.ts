/**
 * Find Usages (Alt+F7), Show Usages (Ctrl+Alt+F7, Ctrl+Click on a declaration) and Find
 * Usages in File (Ctrl+F7): the usages come from Rust (`code_find_usages`: language
 * servers, else the PHP index, filtered by the Find Usages options and scope), grouped by
 * usage type and file there; the popup and the Find tool window only render them.
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { editor, isDirty } from "@/state/editor";
import { onProjectSwitch } from "@/state/workspace";
import { showToolWindow } from "@/state/layout";
import { openFileAt } from "@/editor/views";
import { showBalloon } from "@/keymap";
import { getSetting } from "@/state/settings";
import type { LspPosition } from "@/ipc/lsp";
import {
  codeApi,
  isLibraryPath,
  type SearchScope,
  type UsageRow,
  type UsagesAnswer,
  type UsageTree,
} from "@/ipc/codeintel";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";
import { openFindUsagesSettings, openUsageSettingsAt, usageOptions, usageScope } from "./findSettings";

/** Where a search started, to run it again (Rerun, Settings). */
export interface UsagesOrigin {
  path: string;
  pos: LspPosition;
}

export interface UsagesPopupState {
  answer: UsagesAnswer;
  x: number;
  y: number;
  /** Where Show Usages ran (the popup's Settings button runs it again). */
  origin?: { path: string; view: EditorView; pos: number };
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

/** Find Usages at an LSP position with the Find Usages settings (or the scope `kind`). */
export async function searchUsages(origin: UsagesOrigin, content?: string, scope?: SearchScope): Promise<UsagesAnswer> {
  await syncNow(origin.path);
  return codeApi.findUsages(origin.path, origin.pos, content, dirtyBuffers(), usageOptions(), usageScope(origin.path, scope));
}

/**
 * Framework elements a language server does not know (WordPress hooks): a provider answers
 * Find Usages / Show Usages for the caret, or returns null to leave it to the servers.
 */
export type UsagesProvider = (path: string, view: EditorView, pos: number, scope?: SearchScope) => Promise<UsagesAnswer | null>;
const usagesProviders: UsagesProvider[] = [];
/** Answers that came from a provider: Rerun asks the provider again. */
const providerAnswers = new WeakSet<UsagesAnswer>();

export function registerUsagesProvider(provider: UsagesProvider): void {
  usagesProviders.push(provider);
}

async function queryUsages(path: string, view: EditorView, pos: number, scope?: SearchScope): Promise<UsagesAnswer | null> {
  if (isLibraryPath(path)) {
    showHint(view, pos, "Usages are searched in project files only");
    return null;
  }
  for (const provider of usagesProviders) {
    const answer = await provider(path, view, pos, scope).catch(() => null);
    if (answer) {
      providerAnswers.add(answer);
      return answer;
    }
  }
  try {
    return await searchUsages({ path, pos: toLsp(view.state.doc, pos) }, view.state.doc.toString(), scope);
  } catch (e) {
    showHint(view, pos, String(e instanceof Error ? e.message : e));
    return null;
  }
}

/** "No usages of User found in Project Files" */
const noUsages = (answer: UsagesAnswer) => `No usages of ${answer.title || "the symbol"} found in ${answer.scope ?? "Project Files"}`;

/**
 * Show Usages popup below `pos`. With `jumpIfSingle` (Go to Declaration on a declaration)
 * a single usage opens directly, like PHPStorm.
 */
export async function showUsagesAt(path: string, view: EditorView, pos: number, opts: { jumpIfSingle?: boolean } = {}): Promise<void> {
  const answer = await queryUsages(path, view, pos);
  if (!answer) return;
  const items = usageItems(answer.byFile);
  if (!items.length) {
    showHint(view, pos, noUsages(answer));
    return;
  }
  if (opts.jumpIfSingle && items.length === 1) {
    openUsage(items[0]);
    return;
  }
  const coords = view.coordsAtPos(pos);
  setUsagesPopup({ answer, x: coords?.left ?? 200, y: (coords?.bottom ?? 200) + 4, origin: { path, view, pos } });
}

/** The popup's Settings button: the Find Usages dialog, then Show Usages again. */
export function showUsagesSettings(popup: UsagesPopupState): void {
  const o = popup.origin;
  closeUsagesPopup();
  if (o) void openFindUsagesSettings(o.path, o.view, o.pos, () => void showUsagesAt(o.path, o.view, o.pos));
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
  /** Where a usages search started (the Settings button). */
  origin?: UsagesOrigin;
  /** Buttons below the results (Structural Replace: Replace All). */
  actions?: FindTabAction[];
}

export interface FindTabAction {
  label: string;
  run: (tab: FindTab) => void | Promise<void>;
  primary?: boolean;
}

export interface FindTabOptions {
  kind?: FindTab["kind"];
  title?: string;
  header?: string;
  rerun?: FindTab["rerun"];
  origin?: UsagesOrigin;
  actions?: FindTabAction[];
  /** Open a new tab even when "Open in new tab" is off. */
  newTab?: boolean;
}

const [findTabs, setFindTabs] = createSignal<FindTab[]>([]);
const [activeFindTab, setActiveFindTab] = createSignal<number | null>(null);
export { findTabs, activeFindTab, setActiveFindTab };

/**
 * Id of the "Find in Files" tab: the live search form with Replace (`features/find`), shown
 * before the pinned result tabs once a search is opened in the Find tool window.
 */
export const SEARCH_TAB = 0;
const [searchTabOpen, setSearchTabOpen] = createSignal(false);
export { searchTabOpen };

// The Find tool window's tabs are about the project they searched.
onProjectSwitch(() => {
  setFindTabs([]);
  setActiveFindTab(null);
  setSearchTabOpen(false);
});

/** Show the Find in Files tab of the Find tool window. */
export function showSearchTab(): void {
  setSearchTabOpen(true);
  setActiveFindTab(SEARCH_TAB);
  showToolWindow("find");
}

let nextTab = 1;

/**
 * Pin results in the Find tool window and show it: a new tab, or (Find Usages with "Open in
 * new tab" off) in place of the active usages tab, like PHPStorm.
 */
export function openInFindWindow(answer: UsagesAnswer, opts: FindTabOptions = {}): void {
  const tab: FindTab = {
    id: nextTab++,
    kind: opts.kind ?? "usages",
    title: opts.title ?? `Usages of ${answer.title}`,
    header: opts.header ?? `Usages of ${answer.title} in ${answer.scope ?? "Project Files"}`,
    answer,
    rerun: opts.rerun,
    origin: opts.origin,
    actions: opts.actions,
  };
  const reuse =
    tab.kind === "usages" && !opts.newTab && !getSetting<boolean>("findUsages.openInNewTab", true)
      ? findTabs().find((t) => t.id === activeFindTab() && t.kind === "usages")
      : undefined;
  if (reuse) setFindTabs((tabs) => tabs.map((t) => (t.id === reuse.id ? tab : t)));
  else setFindTabs((tabs) => [...tabs.slice(-9), tab]);
  setActiveFindTab(tab.id);
  closeUsagesPopup();
  showToolWindow("find");
}

export function closeFindTab(id: number): void {
  if (id === SEARCH_TAB) setSearchTabOpen(false);
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
    showHint(view, pos, noUsages(answer));
    return;
  }
  const origin = { path, pos: toLsp(view.state.doc, pos) };
  const rerun = providerAnswers.has(answer)
    ? async () => (await queryUsages(path, view, pos)) ?? answer
    : () => searchUsages(origin);
  openInFindWindow(answer, { origin, rerun });
}

/** Find Usages in File (Ctrl+F7): the usages in the current file, in the Find tool window. */
export async function findUsagesInFileAt(path: string, view: EditorView, pos: number): Promise<void> {
  const answer = await queryUsages(path, view, pos, "currentFile");
  if (!answer) return;
  if (!answer.byFile.total) {
    showHint(view, pos, noUsages(answer));
    return;
  }
  const origin = { path, pos: toLsp(view.state.doc, pos) };
  const file = path.split("/").pop() ?? path;
  openInFindWindow(answer, {
    title: `Usages of ${answer.title} in ${file}`,
    rerun: () => searchUsages(origin, undefined, "currentFile"),
  });
}

/**
 * The Find tool window's Settings button: the Find Usages dialog for the tab's element,
 * then Find Usages again (in a new tab or this one, as "Open in new tab" says).
 */
export function findUsagesSettingsFor(tab: FindTab): void {
  const origin = tab.origin;
  if (!origin) return;
  const content = editor.tabs.find((t) => t.path === origin.path)?.content;
  void openUsageSettingsAt(origin.path, origin.pos, content, async () => {
    try {
      const answer = await searchUsages(origin, content);
      openInFindWindow(answer, { origin, rerun: () => searchUsages(origin) });
    } catch (e) {
      showBalloon("Find Usages failed", String(e instanceof Error ? e.message : e));
    }
  });
}
