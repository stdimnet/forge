import { createStore, produce } from "solid-js/store";
import { workspaceApi } from "@/ipc/workspace";

export interface EditorTab {
  path: string;
  name: string;
  content: string;
  /** Content as last loaded/saved; used for the dirty marker. */
  saved: string;
  cursor: { line: number; col: number };
}

interface EditorState {
  tabs: EditorTab[];
  active: string | null;
}

const [editor, setEditor] = createStore<EditorState>({ tabs: [], active: null });

export { editor };

export const activeTab = () => editor.tabs.find((t) => t.path === editor.active);
export const isDirty = (t: EditorTab) => t.content !== t.saved;

export async function openFile(path: string): Promise<void> {
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

export async function saveFile(path: string): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab) return;
  await workspaceApi.writeFile(path, tab.content);
  setEditor("tabs", (t) => t.path === path, "saved", tab.content);
}

export async function saveAll(): Promise<void> {
  await Promise.all(editor.tabs.filter(isDirty).map((t) => saveFile(t.path)));
}

/** Re-read an open file from disk after an external change (unless it has unsaved edits). */
export async function reloadFile(path: string): Promise<void> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || isDirty(tab)) return;
  const content = await workspaceApi.readFile(path);
  setEditor("tabs", (t) => t.path === path, { content, saved: content });
}
