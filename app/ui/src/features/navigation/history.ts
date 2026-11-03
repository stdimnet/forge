/**
 * Navigation history: recent files (Ctrl+E), edited files, Back/Forward over file
 * switches (Ctrl+Alt+Left/Right) and recently closed tabs.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { editor, isCustomTab, isDirty, onTabsReset, openFile } from "@/state/editor";

const MAX_RECENT = 50;

const [recentFiles, setRecentFiles] = createSignal<string[]>([]);
const [editedFiles, setEditedFiles] = createSignal<string[]>([]);
const [closedTabs, setClosedTabs] = createSignal<string[]>([]);
const back: string[] = [];
const forward: string[] = [];
let navigating = false;

export { recentFiles, editedFiles, closedTabs };

const bump = (list: string[], path: string) => [path, ...list.filter((p) => p !== path)].slice(0, MAX_RECENT);

createRoot(() => {
  createEffect(
    on(
      () => editor.active,
      (path, prev) => {
        if (!path || isCustomTab(path)) return;
        setRecentFiles((l) => bump(l, path));
        if (!navigating && prev && prev !== path && !isCustomTab(prev)) {
          back.push(prev);
          forward.length = 0;
        }
      },
    ),
  );
  createEffect(() => {
    for (const t of editor.tabs) if (isDirty(t) && !editedFiles().includes(t.path)) setEditedFiles((l) => bump(l, t.path));
  });
  createEffect(
    on(
      () => editor.tabs.map((t) => t.path),
      (now, before) => {
        if (!before) return;
        const closed = before.filter((p) => !now.includes(p));
        if (closed.length) setClosedTabs((l) => [...closed, ...l].slice(0, MAX_RECENT));
      },
    ),
  );
});

// The paths belong to the project that was open; another project (a Code With Me host's, an
// SSH host's) must not inherit them.
onTabsReset(() => {
  setRecentFiles([]);
  setEditedFiles([]);
  setClosedTabs([]);
  back.length = 0;
  forward.length = 0;
});

async function go(from: string[], to: string[]): Promise<void> {
  while (from.length) {
    const target = from.pop()!;
    if (target === editor.active) continue;
    if (editor.active) to.push(editor.active);
    navigating = true;
    try {
      await openFile(target);
    } catch {
      continue;
    } finally {
      navigating = false;
    }
    return;
  }
}

export const navigateBack = () => go(back, forward);
export const navigateForward = () => go(forward, back);

export async function reopenClosedTab(): Promise<void> {
  const [first, ...rest] = closedTabs();
  if (!first) return;
  setClosedTabs(rest);
  await openFile(first);
}

export function forgetRecent(path: string): void {
  setRecentFiles((l) => l.filter((p) => p !== path));
  setEditedFiles((l) => l.filter((p) => p !== path));
}
