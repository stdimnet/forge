import { createSignal } from "solid-js";
import { workspaceApi, type WorkspaceInfo } from "@/ipc/workspace";
import { remoteApi } from "@/ipc/remote";
import { isTauri } from "@/ipc";
import { setGuestRoot } from "@/ipc/projectPath";
import { closeAllTabs } from "./editor";

const sameWorkspace = (a: WorkspaceInfo | null, b: WorkspaceInfo | null) =>
  a === b || (!!a && !!b && JSON.stringify(a) === JSON.stringify(b));

// An equal project is not set again: a guest gets the session's project with every status
// update, and everything that follows `workspace()` would reload each time.
const [workspace, setWorkspaceSignal] = createSignal<WorkspaceInfo | null>(null, { equals: sameWorkspace });

function setWorkspace(info: WorkspaceInfo | null): void {
  setGuestRoot(info?.collab ? info.root : null);
  setWorkspaceSignal(info);
}

export { workspace };

/** The open project lives on a remote host (opened over SSH). */
export const isRemoteProject = () => !!workspace()?.remote;

/** The open project is the host's in a Code With Me session (this window is a guest). */
export const isCollabGuest = () => !!workspace()?.collab;

/** Show a project the backend opened (e.g. by a dialog window) in this window. */
export function adoptWorkspace(info: WorkspaceInfo): void {
  adopt(info);
}

type ProjectSwitchListener = () => void;
const switchListeners = new Set<ProjectSwitchListener>();

/**
 * Drop per-project UI state (tree expansion, problems, search results) when this window's
 * project changes: another project opens in this window, a Code With Me guest gets the
 * host's project, or the project closes. Runs before the new project becomes current.
 */
export function onProjectSwitch(listener: ProjectSwitchListener): () => void {
  switchListeners.add(listener);
  return () => switchListeners.delete(listener);
}

function projectSwitched(): void {
  closeAllTabs();
  for (const listener of [...switchListeners]) {
    try {
      listener();
    } catch (e) {
      console.warn("resetting project state failed", e);
    }
  }
}

function adopt(info: WorkspaceInfo): void {
  if (workspace()?.root !== info.root) {
    // From the Welcome screen there is nothing of another project to drop.
    if (workspace()) projectSwitched();
    else closeAllTabs();
  }
  setWorkspace(info);
}

/** The answer to "Trust and Open Project?": open trusted, open in Safe Mode, or not at all. */
export type TrustDecision = "trust" | "safe" | "cancel";

/** "Trust and Open Project?" (registered by the trust feature). */
export interface TrustPrompt {
  /** Before a local folder opens; resolves at once for trusted folders. */
  beforeOpen(path: string): Promise<TrustDecision>;
  /** After a remote project opened (its folder is only known then); it is in Safe Mode until trusted. */
  afterRemoteOpen(): Promise<TrustDecision>;
}

let trustPrompt: TrustPrompt | null = null;

export function setTrustPrompt(prompt: TrustPrompt): void {
  trustPrompt = prompt;
}

/** Opens a folder through another backend; resolves `false` for paths it does not serve. */
export type PathHandler = (path: string) => Promise<boolean>;

const pathHandlers: PathHandler[] = [];

/** Let a feature open some paths itself (WSL opens `\\wsl$\…` folders in the distribution). */
export function registerPathHandler(handler: PathHandler): void {
  pathHandlers.push(handler);
}

/**
 * Open a local folder as the project. An untrusted folder asks first (Trust Project,
 * Preview in Safe Mode, Don't Open); `askTrust: false` skips that for a folder that is
 * open already (Repair IDE reopens it).
 */
export async function openWorkspace(path: string, options: { askTrust?: boolean } = {}): Promise<boolean> {
  // Paths another backend serves (`\\wsl$\<distro>\…` opens in WSL).
  for (const handler of pathHandlers) if (await handler(path)) return true;
  if (options.askTrust !== false && trustPrompt && (await trustPrompt.beforeOpen(path)) === "cancel") return false;
  adopt(await workspaceApi.open(path));
  return true;
}

/** Open `path` on a host connected with `remoteApi.connect` as the project. */
export async function openRemoteProject(connectionId: string, path: string): Promise<void> {
  adopt(await remoteApi.openProject(connectionId, path));
  if (trustPrompt && (await trustPrompt.afterRemoteOpen()) === "cancel") {
    // Don't Open: nothing of the project ran (it was in Safe Mode); leave it again.
    closeWorkspace();
    await remoteApi.disconnect(connectionId).catch(() => undefined);
  }
}

/**
 * Show a project the backend opened in a container or a WSL distribution and ask "Trust and
 * Open Project?" when it is untrusted (it is in Safe Mode until then). Resolves `false` for
 * Don't Open.
 */
export async function adoptRemoteWorkspace(info: WorkspaceInfo): Promise<boolean> {
  adopt(info);
  if (trustPrompt && (await trustPrompt.afterRemoteOpen()) === "cancel") {
    closeWorkspace();
    return false;
  }
  return true;
}

/** Adopt the project the backend already has open (passed on the command line). */
export async function restoreWorkspace(): Promise<void> {
  const current = await workspaceApi.current();
  if (current) setWorkspace(current);
}

/** "Trust and Open Project?" for a folder another window will open (resolves at once when trusted). */
export async function trustBeforeOpen(path: string): Promise<TrustDecision> {
  return trustPrompt ? trustPrompt.beforeOpen(path) : "trust";
}

type ProjectOpener = (path: string) => Promise<unknown>;
let opener: ProjectOpener = openWorkspace;

/**
 * How File | Open and the recent projects open a local project. The project windows feature
 * sets it to ask "New Window / This Window" while a project is open.
 */
export function setProjectOpener(fn: ProjectOpener): void {
  opener = fn;
}

/** Open a local project the way the user wants (this window or a new one). */
export const openProject = async (path: string): Promise<void> => void (await opener(path));

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
  projectSwitched();
  setWorkspace(null);
}
