/**
 * Changes made to project files outside the editor (git, composer, generators, other
 * tools), reported by the backend's debounced file watcher as `fs://changed`. Ignored
 * paths (`.gitignore`, `.git`, `node_modules`, ...) are already filtered out; `git` says
 * that git's own state (index, HEAD, refs) changed.
 */
import { listen } from "@/ipc";
import { editor, reloadFile } from "./editor";

export const FS_CHANGED_EVENT = "fs://changed";

export interface FsChanges {
  /** Created or modified paths (workspace-relative). */
  changed: string[];
  /** Paths that no longer exist. */
  removed: string[];
  git: boolean;
}

type Listener = (c: FsChanges) => void;
const listeners = new Set<Listener>();
let started = false;

function start(): void {
  if (started) return;
  started = true;
  void listen<FsChanges>(FS_CHANGED_EVENT, (c) => dispatchFsChanges(c));
}

/** Subscribe to external file changes. */
export function onFsChanged(cb: Listener): () => void {
  start();
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** Deliver a batch to all subscribers (exported for tests and browser-mode mocks). */
export function dispatchFsChanges(c: FsChanges): void {
  for (const l of listeners) {
    try {
      l(c);
    } catch (e) {
      console.warn("fs change listener failed", e);
    }
  }
}

export const parentDir = (path: string): string => path.split("/").slice(0, -1).join("/");

/** Directories whose listing may have changed: the parents of every changed path. */
export function affectedDirs(c: FsChanges): string[] {
  return [...new Set([...c.changed, ...c.removed].map(parentDir))];
}

/** Reload open tabs whose file changed on disk; tabs with unsaved edits are kept. */
export function startReloadingChangedTabs(): void {
  onFsChanged((c) => {
    const changed = new Set(c.changed);
    for (const tab of editor.tabs) {
      if (changed.has(tab.path)) void reloadFile(tab.path).catch(() => undefined);
    }
  });
}
