import { batch, createSignal, type Component } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { workspaceApi } from "@/ipc/workspace";
import { getSetting } from "./settings";
import * as groups from "./tabGroups";
import type { ActivateOnClose, SplitOrientation, TabLayout, TabPolicy } from "./tabGroups";

export interface EditorTab {
  path: string;
  name: string;
  content: string;
  /** Content as last loaded/saved; used for the dirty marker. */
  saved: string;
  /** Line ending of the file on disk. Buffers always use `\n`; saving converts back. */
  eol: Eol;
  cursor: { line: number; col: number };
  /**
   * Tabs that are not files (SQL consoles, table editors) name the editor view that renders
   * them (`registerEditorView`); their path is a pseudo URI like `db-console://shop/1`.
   */
  view?: string;
}

interface EditorState {
  /** Open documents, one per file, shared by every editor group that shows the file. */
  tabs: EditorTab[];
  /** The active tab of the active editor group. */
  active: string | null;
  /** Editor groups (split editors), tab order and pinned tabs (`state/tabGroups.ts`). */
  layout: TabLayout;
}

export type Eol = "\n" | "\r\n";

/** Split a file's text into `\n`-only text (what the editor holds) and its line ending. */
export function normalizeEol(raw: string): { text: string; eol: Eol } {
  const crlf = (raw.match(/\r\n/g) ?? []).length;
  const lf = (raw.match(/(?<!\r)\n/g) ?? []).length;
  return { text: raw.replace(/\r\n?/g, "\n"), eol: crlf > lf ? "\r\n" : "\n" };
}

const withEol = (text: string, eol: Eol) => (eol === "\r\n" ? text.replaceAll("\n", "\r\n") : text);

const [editor, setEditor] = createStore<EditorState>({ tabs: [], active: null, layout: groups.newLayout() });

export { editor };

export const activeTab = () => editor.tabs.find((t) => t.path === editor.active);

/**
 * Files outside the project (scratches) live under a path prefix such as `scratch://` and are
 * read and written by the feature that owns them instead of the workspace.
 */
export interface FileSystemProvider {
  prefix: string;
  read: (path: string) => Promise<string>;
  write: (path: string, content: string) => Promise<void>;
  /** Tab icon instead of the file type icon. */
  icon?: Component<{ size?: number; name: string }>;
  /** Breadcrumbs above the editor. */
  breadcrumbs?: (path: string) => string[];
}

const fileSystems: FileSystemProvider[] = [];

export function registerFileSystem(provider: FileSystemProvider): void {
  fileSystems.push(provider);
}

export const fileSystemFor = (path: string): FileSystemProvider | undefined => fileSystems.find((f) => path.startsWith(f.prefix));

const readText = (path: string) => (fileSystemFor(path)?.read ?? workspaceApi.readFile)(path);
const writeText = (path: string, text: string) => (fileSystemFor(path)?.write ?? workspaceApi.writeFile)(path, text);

/** View tabs are not files, except those backed by a file system provider (a SQL scratch in a console). */
const isFileTab = (t: EditorTab) => !t.view || !!fileSystemFor(t.path);

export const isDirty = (t: EditorTab) => isFileTab(t) && t.content !== t.saved;

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

type CloseListener = (tab: EditorTab) => void;
const closeListeners = new Set<CloseListener>();

/** Notified when a tab is closed (view tabs release their resources). */
export function onTabClosed(cb: CloseListener): () => void {
  closeListeners.add(cb);
  return () => closeListeners.delete(cb);
}

// ---- Editor groups, pinned tabs and the closing policy --------------------------------

/** The closing policy from Settings | Editor | General | Editor Tabs. */
export function tabPolicy(): TabPolicy {
  return {
    limit: Number(getSetting<number>("editor.tabs.limit", 10)),
    closeUnmodifiedFirst: getSetting<boolean>("editor.tabs.closeUnmodifiedFirst", true) !== false,
    activateOnClose: getSetting<ActivateOnClose>("editor.tabs.activateOnClose", "recent"),
    isDirty: (id) => {
      const t = editor.tabs.find((x) => x.path === id);
      return !!t && isDirty(t);
    },
  };
}

/**
 * Store a new layout: documents and custom tabs no group shows any more are closed (close
 * listeners run) and `editor.active` follows the active group.
 */
function commitLayout(next: TabLayout): void {
  const open = groups.openIds(next);
  const closing = editor.tabs.filter((t) => !open.has(t.path));
  if (customTabs().some((t) => !open.has(t.id))) setCustomTabs((tabs) => tabs.filter((t) => open.has(t.id)));
  // `reconcile` keeps the group objects (keyed by id), so editors in unchanged groups stay mounted.
  batch(() => {
    if (next !== editor.layout) setEditor("layout", reconcile(next));
    if (closing.length) setEditor("tabs", (tabs) => tabs.filter((t) => open.has(t.path)));
    setEditor("active", groups.activeTabId(next));
  });
  for (const t of closing) closeListeners.forEach((l) => l(t));
}

/** Show a loaded tab in a group (default: the active one) and activate it. */
function showTab(id: string, group?: number): void {
  commitLayout(groups.openTab(editor.layout, id, { group, policy: tabPolicy() }).layout);
}

export const editorGroups = () => editor.layout.groups;
export const activeGroupId = () => editor.layout.activeGroup;
export const splitOrientation = (): SplitOrientation => editor.layout.orientation;
export const isTabPinned = (id: string) => editor.layout.pinned.includes(id);
/** The active tab of a group. */
export const groupActive = (group: number) => groups.groupById(editor.layout, group)?.active ?? null;

/** Activate a tab of a group (a click on the tab). */
export function activateTab(group: number, id: string): void {
  commitLayout(groups.activateTab(editor.layout, group, id));
}

/** Make a group the active one (focus moved into it). */
export function focusGroup(group: number): void {
  if (editor.layout.activeGroup === group || !groups.groupById(editor.layout, group)) return;
  commitLayout({ ...editor.layout, activeGroup: group });
}

/** Close tabs of one group (`null`: of every group); named pinned tabs close too. */
export function closeTabsIn(group: number | null, ids: string[]): void {
  if (!ids.length) return;
  commitLayout(groups.closeTabs(editor.layout, group, ids, tabPolicy()));
}

/** Close one tab of a group (its close button, Ctrl+F4). */
export const closeTabIn = (group: number, id: string) => closeTabsIn(group, [id]);

/** Close Other Tabs (pinned tabs stay). */
export const closeOtherTabs = (group: number, id: string) => closeTabsIn(group, groups.othersOf(editor.layout, group, id));
/** Close All Tabs of a group (pinned tabs stay). */
export const closeAllTabsIn = (group: number) => closeTabsIn(group, groups.allOf(editor.layout, group));
/** Close Tabs to the Left / Right (pinned tabs stay). */
export const closeTabsToSide = (group: number, id: string, side: "left" | "right") =>
  closeTabsIn(group, groups.sideOf(editor.layout, group, id, side));

/** Close Unmodified Tabs (pinned tabs stay). */
export function closeUnmodifiedTabs(group: number): void {
  const g = groups.groupById(editor.layout, group);
  const dirty = tabPolicy().isDirty;
  closeTabsIn(group, (g?.tabs ?? []).filter((id) => !dirty(id) && !isTabPinned(id)));
}

export function togglePinTab(id: string): void {
  commitLayout(groups.togglePin(editor.layout, id));
}

/** Split Right (`vertical`) / Split Down (`horizontal`) with a tab (default: the active one). */
export function splitEditor(orientation: SplitOrientation, group?: number, id?: string): void {
  commitLayout(groups.split(editor.layout, orientation, group, id));
}

/** Move a tab to the neighbouring editor group. */
export function moveTabToOppositeGroup(group: number, id: string): void {
  commitLayout(groups.moveToOpposite(editor.layout, group, id, tabPolicy()));
}

/** Unsplit one group into its neighbour, or every group without an argument. */
export function unsplitEditor(group?: number): void {
  commitLayout(groups.unsplit(editor.layout, group));
}

export function changeSplitOrientation(): void {
  commitLayout({ ...editor.layout, orientation: editor.layout.orientation === "vertical" ? "horizontal" : "vertical" });
}

/** Next/previous tab of the active group (Alt+Right / Alt+Left). */
export function cycleTabs(delta: number): void {
  const next = groups.cycle(editor.layout, editor.layout.activeGroup, delta);
  if (next) activateTab(editor.layout.activeGroup, next);
}

/** Goto Next/Previous Splitter: activate the neighbouring group; returns its id. */
export function focusNeighbourGroup(delta: number): number | undefined {
  const g = groups.neighbourGroup(editor.layout, delta);
  if (g) focusGroup(g.id);
  return g?.id;
}

/** Open (or focus) a custom tab; an existing tab with the same id keeps its position. */
export function openCustomTab(tab: CustomTab): void {
  if (!tab.id.startsWith(CUSTOM_TAB_PREFIX)) throw new Error(`custom tab ids start with ${CUSTOM_TAB_PREFIX}`);
  setCustomTabs((tabs) => (tabs.some((t) => t.id === tab.id) ? tabs.map((t) => (t.id === tab.id ? tab : t)) : [...tabs, tab]));
  showTab(tab.id, groups.groupOf(editor.layout, tab.id)?.id);
}

/** Open a file (or activate its tab) in an editor group; default: the active group. */
export async function openFile(path: string, opts: { group?: number } = {}): Promise<void> {
  if (isCustomTab(path)) {
    if (customTabs().some((t) => t.id === path)) showTab(path, opts.group ?? groups.groupOf(editor.layout, path)?.id);
    return;
  }
  if (!editor.tabs.some((t) => t.path === path)) {
    const { text: content, eol } = normalizeEol(await readText(path));
    const name = path.split("/").pop() ?? path;
    if (!editor.tabs.some((t) => t.path === path)) {
      setEditor("tabs", (tabs) => [...tabs, { path, name, content, saved: content, eol, cursor: { line: 1, col: 1 } }]);
    }
  }
  showTab(path, opts.group);
}

/** Open (or activate) a tab rendered by a registered editor view instead of a file. */
export function openViewTab(tab: { path: string; name: string; view: string; content?: string }): void {
  if (!editor.tabs.some((t) => t.path === tab.path)) {
    const content = tab.content ?? "";
    setEditor("tabs", (tabs) => [
      ...tabs,
      { path: tab.path, name: tab.name, view: tab.view, content, saved: content, eol: "\n", cursor: { line: 1, col: 1 } },
    ]);
  }
  showTab(tab.path, groups.groupOf(editor.layout, tab.path)?.id);
}

/** Rename a view tab (e.g. a console after its data source was renamed). */
export function renameTab(path: string, name: string): void {
  setEditor("tabs", (t) => t.path === path, "name", name);
}

/** Close a file (or custom tab) in every editor group. */
export function closeTab(path: string): void {
  if (groups.openIds(editor.layout).has(path)) closeTabsIn(null, [path]);
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
  if (!tab || !isFileTab(tab)) return;
  const content = tab.content;
  await writeText(path, withEol(content, tab.eol));
  setEditor("tabs", (t) => t.path === path, "saved", content);
  saveListeners.forEach((l) => l(path, content));
}

export async function saveAll(): Promise<void> {
  await Promise.all(editor.tabs.filter(isDirty).map((t) => saveFile(t.path)));
}

/** Re-read an open file from disk after an external change (unless it has unsaved edits). */
export async function reloadFile(path: string): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || !isFileTab(tab) || isDirty(tab)) return;
  const { text: content, eol } = normalizeEol(await readText(path));
  setEditor("tabs", (t) => t.path === path, { content, saved: content, eol });
}

/** Close every tab, e.g. when switching projects. */
export function closeAllTabs(): void {
  const closing = [...editor.tabs];
  setCustomTabs([]);
  setEditor({ tabs: [], active: null });
  setEditor("layout", reconcile(groups.newLayout()));
  for (const t of closing) closeListeners.forEach((l) => l(t));
}

/** Adopt the line ending of a file that was rewritten on disk (rollback, checkout, ...). */
export function setTabEol(path: string, eol: Eol): void {
  setEditor("tabs", (t) => t.path === path, "eol", eol);
}

type MoveListener = (from: string, to: string | null) => void;
const moveListeners = new Set<MoveListener>();

/**
 * Notified when a project file or folder was renamed or moved (`to`) or deleted (`to` is
 * null) from within Forge, so bookmarks and similar per-path state can follow it.
 */
export function onPathMoved(cb: MoveListener): () => void {
  moveListeners.add(cb);
  return () => moveListeners.delete(cb);
}

export function notifyPathMoved(from: string, to: string | null): void {
  moveListeners.forEach((l) => l(from, to));
}
