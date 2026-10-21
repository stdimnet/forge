import { createSignal, type Component } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { workspaceApi } from "@/ipc/workspace";

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
  /**
   * The text is a shared document (Code With Me): edits arrive through the editor binding,
   * saving goes through the session, and `savedHash` (not `saved`) says what is on disk.
   */
  shared?: boolean;
  /** `contentHash` of the text on disk, for shared tabs. */
  savedHash?: string;
}

interface EditorState {
  tabs: EditorTab[];
  active: string | null;
}

export type Eol = "\n" | "\r\n";

/** Split a file's text into `\n`-only text (what the editor holds) and its line ending. */
export function normalizeEol(raw: string): { text: string; eol: Eol } {
  const crlf = (raw.match(/\r\n/g) ?? []).length;
  const lf = (raw.match(/(?<!\r)\n/g) ?? []).length;
  return { text: raw.replace(/\r\n?/g, "\n"), eol: crlf > lf ? "\r\n" : "\n" };
}

const withEol = (text: string, eol: Eol) => (eol === "\r\n" ? text.replaceAll("\n", "\r\n") : text);

const [editor, setEditor] = createStore<EditorState>({ tabs: [], active: null });

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

export const isDirty = (t: EditorTab) =>
  isFileTab(t) && (t.savedHash !== undefined ? contentHash(t.content) !== t.savedHash : t.content !== t.saved);

const hashCache = new Map<string, string>();

/**
 * `<UTF-16 length>:<FNV-1a 32, hex>` of a text; the Code With Me host computes the same
 * value in Rust (`forge_collab::protocol::text_hash`) for the saved text of shared files.
 */
export function contentHash(text: string): string {
  const cached = hashCache.get(text);
  if (cached) return cached;
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  const out = `${text.length}:${h.toString(16).padStart(8, "0")}`;
  if (hashCache.size > 64) hashCache.clear();
  hashCache.set(text, out);
  return out;
}

/**
 * Where shared tabs (Code With Me) get their text and how they are saved. `open` answers
 * `undefined` for files that are not shared; they open as usual.
 */
export interface SharedTabSource {
  open: (path: string) => Promise<{ content: string; savedHash: string } | undefined>;
  save: (path: string) => Promise<void>;
}

let sharedTabs: SharedTabSource | null = null;

export function setSharedTabSource(source: SharedTabSource | null): void {
  sharedTabs = source;
}

/** Turn an open tab into a shared one (a session started while it was open). */
export function markTabShared(path: string, savedHash: string): void {
  setEditor("tabs", (t) => t.path === path, { shared: true, savedHash });
}

/** The saved state of a shared tab changed (someone saved, or the file changed on disk). */
export function setSavedHash(path: string, savedHash: string): void {
  setEditor("tabs", (t) => t.path === path && !!t.shared, "savedHash", savedHash);
}

/** A shared tab becomes a normal tab again; `saved` is the text on disk. */
export function unshareTab(path: string, saved: string): void {
  setEditor("tabs", (t) => t.path === path, { shared: false, savedHash: undefined, saved });
}

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
    const shared = await sharedTabs?.open(path);
    const name = path.split("/").pop() ?? path;
    if (shared) {
      if (!editor.tabs.some((t) => t.path === path)) {
        const { content, savedHash } = shared;
        setEditor("tabs", (tabs) => [
          ...tabs,
          { path, name, content, saved: content, eol: "\n", cursor: { line: 1, col: 1 }, shared: true, savedHash },
        ]);
      }
    } else {
      const { text: content, eol } = normalizeEol(await readText(path));
      setEditor("tabs", (tabs) => [
        ...tabs,
        { path, name, content, saved: content, eol, cursor: { line: 1, col: 1 } },
      ]);
    }
  }
  setEditor("active", path);
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
  if (!tab || !isFileTab(tab)) return;
  const content = tab.content;
  if (tab.shared && sharedTabs) {
    // The host writes the shared document; its save state arrives through the session.
    await sharedTabs.save(path);
    saveListeners.forEach((l) => l(path, content));
    return;
  }
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
  // Shared tabs follow the disk through the session (the host applies the change).
  if (!tab || !isFileTab(tab) || isDirty(tab) || tab.shared) return;
  const { text: content, eol } = normalizeEol(await readText(path));
  setEditor("tabs", (t) => t.path === path, { content, saved: content, eol });
}

/** Close every tab, e.g. when switching projects. */
export function closeAllTabs(): void {
  const closing = [...editor.tabs];
  setCustomTabs([]);
  setEditor({ tabs: [], active: null });
  for (const t of closing) closeListeners.forEach((l) => l(t));
}

/** Adopt the line ending of a file that was rewritten on disk (rollback, checkout, ...). */
export function setTabEol(path: string, eol: Eol): void {
  setEditor("tabs", (t) => t.path === path, "eol", eol);
}
