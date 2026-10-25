import { createSignal } from "solid-js";
import { workspaceApi, type WorkspaceInfo } from "@/ipc/workspace";
import { remoteApi } from "@/ipc/remote";
import { isTauri } from "@/ipc";
import { closeAllTabs } from "./editor";

const [workspace, setWorkspace] = createSignal<WorkspaceInfo | null>(null);

export { workspace };

/** The open project lives on a remote host (opened over SSH). */
export const isRemoteProject = () => !!workspace()?.remote;

/** Show a project the backend opened (e.g. by a dialog window) in this window. */
export function adoptWorkspace(info: WorkspaceInfo): void {
  adopt(info);
}

function adopt(info: WorkspaceInfo): void {
  if (workspace()?.root !== info.root) closeAllTabs();
  setWorkspace(info);
}

export async function openWorkspace(path: string): Promise<void> {
  adopt(await workspaceApi.open(path));
}

/** Open `path` on a host connected with `remoteApi.connect` as the project. */
export async function openRemoteProject(connectionId: string, path: string): Promise<void> {
  adopt(await remoteApi.openProject(connectionId, path));
}

/** Adopt the project the backend already has open (passed on the command line). */
export async function restoreWorkspace(): Promise<void> {
  const current = await workspaceApi.current();
  if (current) setWorkspace(current);
}

type ProjectOpener = (path: string) => Promise<void>;
let opener: ProjectOpener = openWorkspace;

/**
 * How File | Open and the recent projects open a local project. The project windows feature
 * sets it to ask "New Window / This Window" while a project is open.
 */
export function setProjectOpener(fn: ProjectOpener): void {
  opener = fn;
}

/** Open a local project the way the user wants (this window or a new one). */
export const openProject = (path: string): Promise<void> => opener(path);

/** Show the native folder picker and open the chosen directory as a project. */
export async function pickAndOpenWorkspace(): Promise<void> {
  if (!isTauri()) {
    await openProject("demo");
    return;
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false, title: "Open File or Project" });
  if (typeof dir === "string") await openProject(dir);
}

export function closeWorkspace(): void {
  closeAllTabs();
  setWorkspace(null);
}
