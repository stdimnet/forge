import { createSignal, type Component } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { textApi, type EncodingId, type Eol, type FileFormat, type SaveOptions, type TextFile } from "@/ipc/text";
import { getSetting } from "@/state/settings";

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

export type { Eol } from "@/ipc/text";

/** Split a file's text into `\n`-only text (what the editor holds) and its line ending. */
export function normalizeEol(raw: string): { text: string; eol: Eol } {
  const crlf = (raw.match(/\r\n/g) ?? []).length;
  const lf = (raw.match(/(?<!\r)\n/g) ?? []).length;
  return { text: raw.replace(/\r\n?/g, "\n"), eol: crlf > lf ? "\r\n" : "\n" };
}

const withEol = (text: string, eol: Eol) => (eol === "\n" ? text : text.replaceAll("\n", eol));

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

/** Read a file. Project files go through Rust, which detects encoding and separator. */
async function readText(path: string, encoding?: EncodingId): Promise<TextFile> {
  const fs = fileSystemFor(path);
  if (!fs) return textApi.read(path, encoding);
  const { text, eol } = normalizeEol(await fs.read(path));
  return { text, eol, encoding: "UTF-8", bom: false, malformed: false };
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

/** Open (or focus) a custom tab; an existing tab with the same id keeps its position. */
export function openCustomTab(tab: CustomTab): void {
  if (!tab.id.startsWith(CUSTOM_TAB_PREFIX)) throw new Error(`custom tab ids start with ${CUSTOM_TAB_PREFIX}`);
  setCustomTabs((tabs) => (tabs.some((t) => t.id === tab.id) ? tabs.map((t) => (t.id === tab.id ? tab : t)) : [...tabs, tab]));
  setEditor("active", tab.id);
}

/** Ids of all open tabs in display order (files first, then custom tabs). */
const allTabIds = () => [...editor.tabs.map((t) => t.path), ...customTabs().map((t) => t.id)];

export async function openFile(path: string, options: { activate?: boolean } = {}): Promise<void> {
  const activate = options.activate ?? true;
  if (isCustomTab(path)) {
    if (activate && customTabs().some((t) => t.id === path)) setEditor("active", path);
    return;
  }
  if (!editor.tabs.some((t) => t.path === path)) {
    const file = await readText(path);
    // Opened twice while reading: keep the first tab.
    if (!editor.tabs.some((t) => t.path === path)) {
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
  if (activate) setEditor("active", path);
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
  setEditor("active", tab.path);
}

/** Activate an open tab (file, view or custom tab). */
export function activateTab(path: string): void {
  if (allTabIds().includes(path)) setEditor("active", path);
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

/**
 * The open editor of a file, as far as saving needs it: the lines with a caret (kept when
 * trailing spaces are removed) and applying the cleaned-up text as an edit. Set by
 * `editor/views.ts`.
 */
export interface EditorBridge {
  caretLines: (path: string) => number[];
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
 * Save a file: remove trailing spaces and add the final line break (Settings | Editor |
 * General | On Save, computed in Rust), then write it with its separator and encoding.
 */
export async function saveFile(path: string): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || !isFileTab(tab)) return;
  let content = tab.content;
  const options = saveOptionsOf(tab);
  if (options.stripTrailingSpaces !== "none" || options.ensureNewlineAtEof) {
    try {
      const result = await textApi.prepareSave({
        text: content,
        saved: tab.saved,
        caretLines: bridge?.caretLines(path) ?? [],
        options,
      });
      // Only when nobody typed meanwhile.
      if (result.changed && editor.tabs.find((t) => t.path === path)?.content === content) {
        content = result.text;
        if (!bridge?.applyText(path, content)) updateContent(path, content);
      }
    } catch (e) {
      console.warn("on-save cleanup failed", e);
    }
  }
  const current = editor.tabs.find((t) => t.path === path);
  if (!current) return;
  await writeTab(current, content);
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
