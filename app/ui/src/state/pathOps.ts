/**
 * Renaming, moving and deleting project files and folders from within Forge. Every operation
 * notifies `onPathMoved` (`state/editor.ts`), so per-path state follows the file: bookmarks,
 * file type overrides, Recent Locations, the edit history and TODO. The file watcher only sees
 * a removed and an added path, which would drop that state instead of moving it.
 */
import { workspaceApi } from "@/ipc/workspace";
import { notifyPathMoved } from "./editor";

/** Rename or move a file or folder (project-relative paths). */
export async function renamePath(from: string, to: string): Promise<void> {
  await workspaceApi.rename(from, to);
  notifyPathMoved(from, to);
}

/** Delete a file or folder with its content. Ask the user first. */
export async function deletePath(path: string): Promise<void> {
  await workspaceApi.delete(path);
  notifyPathMoved(path, null);
}
