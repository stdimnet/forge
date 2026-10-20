import { createSignal } from "solid-js";
import { workspaceApi, type WorkspaceInfo } from "@/ipc/workspace";
import { isTauri } from "@/ipc";
import { closeAllTabs } from "./editor";

const [workspace, setWorkspace] = createSignal<WorkspaceInfo | null>(null);

export { workspace };

export async function openWorkspace(path: string): Promise<void> {
  const info = await workspaceApi.open(path);
  if (workspace()?.root !== info.root) closeAllTabs();
  setWorkspace(info);
}

/** Adopt the project the backend already has open (passed on the command line). */
export async function restoreWorkspace(): Promise<void> {
  const current = await workspaceApi.current();
  if (current) setWorkspace(current);
}

/** Show the native folder picker and open the chosen directory as a project. */
export async function pickAndOpenWorkspace(): Promise<void> {
  if (!isTauri()) {
    await openWorkspace("demo");
    return;
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false, title: "Open File or Project" });
  if (typeof dir === "string") await openWorkspace(dir);
}

export function closeWorkspace(): void {
  closeAllTabs();
  setWorkspace(null);
}
