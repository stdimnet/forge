import { createSignal, type Component } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { workspaceApi } from "@/ipc/workspace";

export interface EditorTab {
  path: string;
  name: string;
  content: string;
  /** Content as last loaded/saved; used for the dirty marker. */
  saved: string;
  cursor: { line: number; col: number };
  /**
   * Tabs that are not files (SQL consoles, table editors) name the editor view that renders
   * them (`registerEditorView`); their path is a pseudo URI like `db-console://shop/1`.
   */
  view?: string;
}

interface EditorState {
  tabs: EditorTab[];
  active: string | null;
}

const [editor, setEditor] = createStore<EditorState>({ tabs: [], active: null });

export { editor };

export const activeTab = () => editor.tabs.find((t) => t.path === editor.active);
export const isDirty = (t: EditorTab) => !t.view && t.content !== t.saved;

/**
 * Editor tabs that show a feature view instead of a file (e.g. a merge request). Their ids
 * start with `forge:` so file-based features can tell them apart; `activeTab()` is
 * undefined while one of them is active.
 */
export interface CustomTab {
  id: string;
  title: string;
  tooltip?: string;
  icon?: Component<{ size?: number }>;
  component: Component;
}

export const CUSTOM_TAB_PREFIX = "forge:";
export const isCustomTab = (id: string | null | undefined): boolean => !!id?.startsWith(CUSTOM_TAB_PREFIX);

const [customTabs, setCustomTabs] = createSignal<CustomTab[]>([]);
export { customTabs };

export const activeCustomTab = () => customTabs().find((t) => t.id === editor.active);

/** Open (or focus) a custom tab; an existing tab with the same id keeps its position. */
export function openCustomTab(tab: CustomTab): void {
  if (!tab.id.startsWith(CUSTOM_TAB_PREFIX)) throw new Error(`custom tab ids start with ${CUSTOM_TAB_PREFIX}`);
  setCustomTabs((tabs) => (tabs.some((t) => t.id === tab.id) ? tabs.map((t) => (t.id === tab.id ? tab : t)) : [...tabs, tab]));
  setEditor("active", tab.id);
}

/** Ids of all open tabs in display order (files first, then custom tabs). */
const allTabIds = () => [...editor.tabs.map((t) => t.path), ...customTabs().map((t) => t.id)];

export async function openFile(path: string): Promise<void> {
  if (isCustomTab(path)) {
    if (customTabs().some((t) => t.id === path)) setEditor("active", path);
    return;
  }
  if (!editor.tabs.some((t) => t.path === path)) {
    const content = await workspaceApi.readFile(path);
    const name = path.split("/").pop() ?? path;
    setEditor("tabs", (tabs) => [
      ...tabs,
      { path, name, content, saved: content, cursor: { line: 1, col: 1 } },
    ]);
  }
  setEditor("active", path);
}

/** Open (or activate) a tab rendered by a registered editor view instead of a file. */
export function openViewTab(tab: { path: string; name: string; view: string; content?: string }): void {
  if (!editor.tabs.some((t) => t.path === tab.path)) {
    const content = tab.content ?? "";
    setEditor("tabs", (tabs) => [
      ...tabs,
      { path: tab.path, name: tab.name, view: tab.view, content, saved: content, cursor: { line: 1, col: 1 } },
    ]);
  }
  setEditor("active", tab.path);
}

/** Rename a view tab (e.g. a console after its data source was renamed). */
export function renameTab(path: string, name: string): void {
  setEditor("tabs", (t) => t.path === path, "name", name);
}

type CloseListener = (tab: EditorTab) => void;
const closeListeners = new Set<CloseListener>();

/** Notified when a tab is closed (view tabs release their resources). */
export function onTabClosed(cb: CloseListener): () => void {
  closeListeners.add(cb);
  return () => closeListeners.delete(cb);
}

export function closeTab(path: string): void {
  const closing = editor.tabs.find((t) => t.path === path);
  const ids = allTabIds();
  const idx = ids.indexOf(path);
  if (idx < 0) return;
  const rest = ids.filter((id) => id !== path);
  if (isCustomTab(path)) setCustomTabs((tabs) => tabs.filter((t) => t.id !== path));
  setEditor(
    produce((s) => {
      const i = s.tabs.findIndex((t) => t.path === path);
      if (i >= 0) s.tabs.splice(i, 1);
      if (s.active === path) s.active = rest[Math.min(idx, rest.length - 1)] ?? null;
    }),
  );
  if (closing) closeListeners.forEach((l) => l(closing));
}

export function updateContent(path: string, content: string): void {
  setEditor("tabs", (t) => t.path === path, "content", content);
}

export function updateCursor(path: string, line: number, col: number): void {
  setEditor("tabs", (t) => t.path === path, "cursor", { line, col });
}

type SaveListener = (path: string, content: string) => void;
const saveListeners = new Set<SaveListener>();

/** Notified after a file was written (e.g. to send `didSave` to language servers). */
export function onFileSaved(cb: SaveListener): () => void {
  saveListeners.add(cb);
  return () => saveListeners.delete(cb);
}

export async function saveFile(path: string): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || tab.view) return;
  const content = tab.content;
  await workspaceApi.writeFile(path, content);
  setEditor("tabs", (t) => t.path === path, "saved", content);
  saveListeners.forEach((l) => l(path, content));
}

export async function saveAll(): Promise<void> {
  await Promise.all(editor.tabs.filter(isDirty).map((t) => saveFile(t.path)));
}

/** Re-read an open file from disk after an external change (unless it has unsaved edits). */
export async function reloadFile(path: string): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || tab.view || isDirty(tab)) return;
  const content = await workspaceApi.readFile(path);
  setEditor("tabs", (t) => t.path === path, { content, saved: content });
}

/** Close every tab, e.g. when switching projects. */
export function closeAllTabs(): void {
  const closing = [...editor.tabs];
  setCustomTabs([]);
  setEditor({ tabs: [], active: null });
  for (const t of closing) closeListeners.forEach((l) => l(t));
}
