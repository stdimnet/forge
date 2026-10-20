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
export const isDirty = (t: EditorTab) => t.content !== t.saved;

export async function openFile(path: string): Promise<void> {
  if (!editor.tabs.some((t) => t.path === path)) {
    const { text: content, eol } = normalizeEol(await workspaceApi.readFile(path));
    const name = path.split("/").pop() ?? path;
    setEditor("tabs", (tabs) => [
      ...tabs,
      { path, name, content, saved: content, eol, cursor: { line: 1, col: 1 } },
    ]);
  }
  setEditor("active", path);
}

export function closeTab(path: string): void {
  setEditor(
    produce((s) => {
      const idx = s.tabs.findIndex((t) => t.path === path);
      if (idx < 0) return;
      s.tabs.splice(idx, 1);
      if (s.active === path) s.active = s.tabs[Math.min(idx, s.tabs.length - 1)]?.path ?? null;
    }),
  );
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
  if (!tab) return;
  const content = tab.content;
  await workspaceApi.writeFile(path, withEol(content, tab.eol));
  setEditor("tabs", (t) => t.path === path, "saved", content);
  saveListeners.forEach((l) => l(path, content));
}

export async function saveAll(): Promise<void> {
  await Promise.all(editor.tabs.filter(isDirty).map((t) => saveFile(t.path)));
}

/** Re-read an open file from disk after an external change (unless it has unsaved edits). */
export async function reloadFile(path: string): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || isDirty(tab)) return;
  const { text: content, eol } = normalizeEol(await workspaceApi.readFile(path));
  setEditor("tabs", (t) => t.path === path, { content, saved: content, eol });
}

/** Close every tab, e.g. when switching projects. */
export function closeAllTabs(): void {
  setEditor({ tabs: [], active: null });
}

/** Adopt the line ending of a file that was rewritten on disk (rollback, checkout, ...). */
export function setTabEol(path: string, eol: Eol): void {
  setEditor("tabs", (t) => t.path === path, "eol", eol);
}
