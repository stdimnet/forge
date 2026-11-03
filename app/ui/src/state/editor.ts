import { batch, createSignal, type Component } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { textApi, type EncodingId, type Eol, type FileFormat, type SaveOptions, type TextFile } from "@/ipc/text";
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
  /** The file had a line break when it was read (else saving uses the code style's separator). */
  eolKnown?: boolean;
  /** Encoding of the file on disk and whether it starts with a byte order mark. */
  encoding: EncodingId;
  bom: boolean;
  /** Effective format (indent, separators, on-save cleanup), resolved in Rust. */
  format?: FileFormat;
  /** Main caret; `carets` counts all carets when there are several. */
  cursor: { line: number; col: number; carets?: number };
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
  /** Open documents, one per file, shared by every editor group that shows the file. */
  tabs: EditorTab[];
  /** The active tab of the active editor group. */
  active: string | null;
  /** Editor groups (split editors), tab order and pinned tabs (`state/tabGroups.ts`). */
  layout: TabLayout;
}

export type { Eol } from "@/ipc/text";

/** Split a file's text into `\n`-only text (what the editor holds) and its line ending. */
export function normalizeEol(raw: string): { text: string; eol: Eol } {
  const crlf = (raw.match(/\r\n/g) ?? []).length;
  const lf = (raw.match(/(?<!\r)\n/g) ?? []).length;
  return { text: raw.replace(/\r\n?/g, "\n"), eol: crlf > lf ? "\r\n" : "\n" };
}

const withEol = (text: string, eol: Eol) => (eol === "\n" ? text : text.replaceAll("\n", eol));

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

/** Read a file. Project files go through Rust, which detects encoding and separator. */
async function readText(path: string, encoding?: EncodingId): Promise<TextFile> {
  const fs = fileSystemFor(path);
  if (!fs) return textApi.read(path, encoding);
  const { text, eol } = normalizeEol(await fs.read(path));
  return { text, eol, encoding: "UTF-8", bom: false, malformed: false };
}

/** An open tab's file as it is on disk now, decoded in the tab's encoding (Git rollback etc.). */
export async function readOpenFile(path: string): Promise<{ text: string; eol: Eol } | undefined> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || !isFileTab(tab)) return undefined;
  const file = await readText(path, tab.encoding === "UTF-8" ? undefined : tab.encoding);
  return { text: file.text, eol: file.eol };
}

/** Write a tab's text with its separator and encoding. */
async function writeTab(tab: EditorTab, text: string): Promise<void> {
  const fs = fileSystemFor(tab.path);
  // A file without a line break when it was read (a new file) gets the code style's separator.
  const eol = tab.eolKnown || !text.includes("\n") ? tab.eol : (tab.format?.lineSeparator ?? tab.eol);
  if (fs) await fs.write(tab.path, withEol(text, eol));
  else await textApi.write(tab.path, text, eol, tab.encoding, tab.bom);
  if (text.includes("\n") && (!tab.eolKnown || eol !== tab.eol)) {
    setEditor("tabs", (t) => t.path === tab.path, { eol, eolKnown: true });
  }
}

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
function showTab(id: string, group?: number, background = false): void {
  commitLayout(groups.openTab(editor.layout, id, { group, policy: tabPolicy(), background }).layout);
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

/**
 * Binary files that open in an editor view instead of as text (images): the tab gets the
 * view id and no text.
 */
export interface BinaryFileView {
  view: string;
  accepts: (path: string) => boolean;
}

const binaryViews: BinaryFileView[] = [];

export function registerBinaryFileView(v: BinaryFileView): void {
  binaryViews.push(v);
}

let forwardOpen: ((path: string) => boolean) | null = null;

/**
 * A window without an editor area (a tool window in its own window) hands the files it
 * opens to its project window; `fn` returns `true` when it took the file.
 */
export function setOpenFileForwarder(fn: ((path: string) => boolean) | null): void {
  forwardOpen = fn;
}

/**
 * Open a file (or activate its tab) in an editor group; default: the active group. With
 * `activate: false` the tab opens in the background (restoring a project's layout).
 */
export async function openFile(path: string, opts: { group?: number; activate?: boolean } = {}): Promise<void> {
  if (forwardOpen?.(path)) return;
  const activate = opts.activate ?? true;
  if (isCustomTab(path)) {
    if (activate && customTabs().some((t) => t.id === path)) showTab(path, opts.group ?? groups.groupOf(editor.layout, path)?.id);
    return;
  }
  const binary = fileSystemFor(path) ? undefined : binaryViews.find((v) => v.accepts(path));
  if (binary && !editor.tabs.some((t) => t.path === path)) {
    const name = path.split("/").pop() ?? path;
    setEditor("tabs", (tabs) => [
      ...tabs,
      { path, name, view: binary.view, content: "", saved: "", eol: "\n", encoding: "UTF-8", bom: false, cursor: { line: 1, col: 1 } },
    ]);
  }
  if (!editor.tabs.some((t) => t.path === path)) {
    const shared = await sharedTabs?.open(path);
    const file = shared ? undefined : await readText(path);
    // Opened twice while reading: keep the first tab.
    if (shared && !editor.tabs.some((t) => t.path === path)) {
      const { content, savedHash } = shared;
      setEditor("tabs", (tabs) => [
        ...tabs,
        {
          path,
          name: path.split("/").pop() ?? path,
          content,
          saved: content,
          eol: "\n",
          eolKnown: content.includes("\n"),
          encoding: "UTF-8",
          bom: false,
          cursor: { line: 1, col: 1 },
          shared: true,
          savedHash,
        },
      ]);
    } else if (file && !editor.tabs.some((t) => t.path === path)) {
      const name = path.split("/").pop() ?? path;
      setEditor("tabs", (tabs) => [
        ...tabs,
        {
          path,
          name,
          content: file.text,
          saved: file.text,
          eol: file.eol,
          eolKnown: file.text.includes("\n"),
          encoding: file.encoding,
          bom: file.bom,
          cursor: { line: 1, col: 1 },
        },
      ]);
    }
  }
  showTab(path, opts.group, !activate);
}

/** Open (or activate) a tab rendered by a registered editor view instead of a file. */
export function openViewTab(tab: { path: string; name: string; view: string; content?: string }): void {
  if (!editor.tabs.some((t) => t.path === tab.path)) {
    const content = tab.content ?? "";
    setEditor("tabs", (tabs) => [
      ...tabs,
      {
        path: tab.path,
        name: tab.name,
        view: tab.view,
        content,
        saved: content,
        eol: "\n",
        encoding: "UTF-8",
        bom: false,
        cursor: { line: 1, col: 1 },
      },
    ]);
  }
  showTab(tab.path, groups.groupOf(editor.layout, tab.path)?.id);
}

/** Activate an open tab (file, view or custom tab) in the group that shows it. */
export function activateOpenTab(path: string): void {
  const group = groups.groupOf(editor.layout, path);
  if (group) showTab(path, group.id);
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

export function updateCursor(path: string, line: number, col: number, carets = 1): void {
  setEditor("tabs", (t) => t.path === path, "cursor", { line, col, carets });
}

type SaveListener = (path: string, content: string, trigger: SaveTrigger) => void;
const saveListeners = new Set<SaveListener>();

/** Notified after a file was written (e.g. to send `didSave` to language servers, automatic upload). */
export function onFileSaved(cb: SaveListener): () => void {
  saveListeners.add(cb);
  return () => saveListeners.delete(cb);
}

/**
 * The open editor of a file, as far as saving needs it: the lines with a caret (kept when
 * trailing spaces are removed) and applying the cleaned-up text as an edit. Set by
 * `editor/views.ts`.
 */
export interface EditorBridge {
  caretLines: (path: string) => number[];
  /** Apply `text` as one edit (one undo step). */
  applyText: (path: string, text: string) => boolean;
}

let bridge: EditorBridge | null = null;

export function setEditorBridge(b: EditorBridge): void {
  bridge = b;
}

/** On-save options of a tab: its resolved format, else the settings. */
function saveOptionsOf(tab: EditorTab): SaveOptions {
  return (
    tab.format?.save ?? {
      stripTrailingSpaces: getSetting<SaveOptions["stripTrailingSpaces"]>("editor.stripTrailingSpaces", "modified"),
      ensureNewlineAtEof: getSetting<boolean>("editor.ensureNewlineAtEof", true),
    }
  );
}

/**
 * What started a save. "explicit" (Ctrl+S) and "autosave" run the Actions on Save
 * (Settings | Tools | Actions on Save); "plain" saves that belong to another operation
 * (refactoring, encoding or line separator changes) do not.
 */
export type SaveTrigger = "explicit" | "autosave" | "plain";

/** The Actions on Save, installed by `features/actionsOnSave`. */
export interface SaveActionsRunner {
  /** The text after the actions that run before writing, and the tools to run after; null: none. */
  before: (path: string, text: string, trigger: "explicit" | "autosave") => Promise<{ text: string; tools: string[] } | null>;
  /** Run the external tools on the written file; true when they changed it. */
  after: (path: string, tools: string[]) => Promise<boolean>;
}

let saveActions: SaveActionsRunner | null = null;

export function setSaveActionsRunner(runner: SaveActionsRunner): void {
  saveActions = runner;
}

/** Saves of one file run one after the other. */
const pendingSaves = new Map<string, Promise<void>>();

/**
 * Save a file: run the Actions on Save, remove trailing spaces and add the final line break
 * (Settings | Editor | General | On Save, computed in Rust), write it with its separator and
 * encoding, then run the external tools on it. Every change lands in the editor as one edit,
 * and only when nobody typed meanwhile, so the user's text is never lost.
 */
export function saveFile(path: string, trigger: SaveTrigger = "plain"): Promise<void> {
  const previous = pendingSaves.get(path) ?? Promise.resolve();
  const next = previous.catch(() => undefined).then(() => saveFileNow(path, trigger));
  pendingSaves.set(path, next);
  next
    .finally(() => {
      if (pendingSaves.get(path) === next) pendingSaves.delete(path);
    })
    .catch(() => undefined);
  return next;
}

async function saveFileNow(path: string, trigger: SaveTrigger): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || !isFileTab(tab)) return;
  if (tab.shared && sharedTabs) {
    // The host writes the shared document; its save state arrives through the session.
    const content = tab.content;
    await sharedTabs.save(path);
    saveListeners.forEach((l) => l(path, content, trigger));
    return;
  }
  const original = tab.content;
  const contentNow = () => editor.tabs.find((t) => t.path === path)?.content;
  let content = original;
  let tools: string[] = [];
  if (trigger !== "plain" && saveActions) {
    try {
      const result = await saveActions.before(path, original, trigger);
      if (result && contentNow() === original) {
        content = result.text;
        tools = result.tools;
      }
    } catch (e) {
      console.warn("actions on save failed", e);
    }
  }
  const options = saveOptionsOf(tab);
  if (options.stripTrailingSpaces !== "none" || options.ensureNewlineAtEof) {
    try {
      const result = await textApi.prepareSave({
        text: content,
        saved: tab.saved,
        // After the actions moved lines around, the caret lines are unknown.
        caretLines: content === original ? (bridge?.caretLines(path) ?? []) : [],
        options,
      });
      if (result.changed) content = result.text;
    } catch (e) {
      console.warn("on-save cleanup failed", e);
    }
  }
  const current = editor.tabs.find((t) => t.path === path);
  if (!current) return;
  // Typed meanwhile: save what is in the editor now, untouched.
  if (current.content !== original) {
    content = current.content;
    tools = [];
  }
  await writeTab(current, content);
  let final = content;
  if (tools.length && saveActions) {
    try {
      if (await saveActions.after(path, tools)) {
        final = (await readText(path, current.encoding === "UTF-8" ? undefined : current.encoding)).text;
      }
    } catch (e) {
      console.warn("actions on save: tools failed", e);
    }
  }
  if (final !== original && contentNow() === original) {
    if (!bridge?.applyText(path, final)) updateContent(path, final);
  }
  setEditor("tabs", (t) => t.path === path, "saved", final);
  saveListeners.forEach((l) => l(path, final, trigger));
}

export async function saveAll(trigger: SaveTrigger = "autosave"): Promise<void> {
  await Promise.all(editor.tabs.filter(isDirty).map((t) => saveFile(t.path, trigger)));
}

/** Re-read an open file from disk after an external change (unless it has unsaved edits). */
export async function reloadFile(path: string): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  // Shared tabs follow the disk through the session (the host applies the change).
  if (!tab || !isFileTab(tab) || isDirty(tab) || tab.shared) return;
  const file = await readText(path, tab.encoding === "UTF-8" ? undefined : tab.encoding);
  if (file.text === tab.content && file.eol === tab.eol && file.bom === tab.bom) return;
  setEditor("tabs", (t) => t.path === path, {
    content: file.text,
    saved: file.text,
    eol: file.eol,
    eolKnown: tab.eolKnown || file.text.includes("\n"),
    encoding: file.encoding,
    bom: file.bom,
  });
}

/** "Reload in another encoding": read the file again, decoding it with `encoding`. */
export async function reloadWithEncoding(path: string, encoding: EncodingId): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || !isFileTab(tab)) return;
  const file = await readText(path, encoding);
  setEditor("tabs", (t) => t.path === path, {
    content: file.text,
    saved: file.text,
    eol: file.eol,
    encoding: file.encoding,
    bom: file.bom,
  });
}

/** "Convert": write the file in another encoding (or with/without BOM) from now on. */
export async function convertEncoding(path: string, encoding: EncodingId, bom: boolean): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || !isFileTab(tab)) return;
  const previous = { encoding: tab.encoding, bom: tab.bom };
  setEditor("tabs", (t) => t.path === path, { encoding, bom });
  try {
    await saveFile(path);
  } catch (e) {
    setEditor("tabs", (t) => t.path === path, previous);
    throw e;
  }
}

/** Change a file's line separator (status bar) and write the file right away, like PHPStorm. */
export async function changeLineSeparator(path: string, eol: Eol): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || !isFileTab(tab) || (tab.eol === eol && tab.eolKnown)) return;
  const previous = { eol: tab.eol, eolKnown: tab.eolKnown };
  setEditor("tabs", (t) => t.path === path, { eol, eolKnown: true });
  try {
    await saveFile(path);
  } catch (e) {
    setEditor("tabs", (t) => t.path === path, previous);
    throw e;
  }
}

/** Store the resolved format of a tab (`editor/fileFormat.ts`). */
export function setTabFormat(path: string, format: FileFormat): void {
  setEditor("tabs", (t) => t.path === path, "format", format);
}

const resetListeners = new Set<() => void>();

/** Notified after `closeAllTabs`, i.e. when another project took the window (per-project state resets). */
export function onTabsReset(cb: () => void): () => void {
  resetListeners.add(cb);
  return () => resetListeners.delete(cb);
}

/** Close every tab, e.g. when switching projects. */
export function closeAllTabs(): void {
  const closing = [...editor.tabs];
  setCustomTabs([]);
  setEditor({ tabs: [], active: null });
  setEditor("layout", reconcile(groups.newLayout()));
  for (const t of closing) closeListeners.forEach((l) => l(t));
  resetListeners.forEach((l) => l());
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
