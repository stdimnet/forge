import { createSignal } from "solid-js";
import { workspaceApi, type WorkspaceInfo } from "@/ipc/workspace";
import { remoteApi } from "@/ipc/remote";
import { isTauri } from "@/ipc";
import { setGuestRoot } from "@/ipc/projectPath";
import { projectLifecycleApi, type LifecycleEvent, type LifecycleProject } from "@/ipc/projectLifecycle";
import { isDialogWindow } from "@/shell/window/mode";
import { closeAllTabs } from "./editor";
import { confirmProjectClose, onProject, runProjectPhase } from "./projectLifecycle";

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

/**
 * Drop per-project UI state when this window's project changes.
 * @deprecated Use `onProject("closing", listener)` from `@/state/projectLifecycle`.
 */
export function onProjectSwitch(listener: () => void): () => void {
  return onProject("closing", () => listener());
}

/** The project whose `opened` handlers ran and whose `closing` handlers did not run yet. */
let current: LifecycleProject | null = null;

/** Dialog windows show a project, but its lifecycle belongs to the project window. */
const runsLifecycle = typeof window === "undefined" || !isDialogWindow();

/** The project the lifecycle handlers know as open (with its VCS root and trust, when Rust sent them). */
export const lifecycleProject = (): LifecycleProject | null => current;

const workspaceInfo = ({ vcsRoot: _v, trusted: _t, ...info }: LifecycleProject): WorkspaceInfo => info;

/** `closing` of the open project: its tabs close, then the handlers run (`workspace()` is still it). */
function leaveCurrent(): void {
  const closing = current;
  if (!closing) return;
  current = null;
  closeAllTabs();
  if (runsLifecycle) void runProjectPhase("closing", closing);
}

/**
 * Make `info` the window's project: `closing` of another one, then `opened` - once per
 * project, whether the IPC result or the Rust event arrives first.
 */
function adopt(info: LifecycleProject): void {
  if (workspace()?.root !== info.root || current?.root !== info.root) {
    if (current) leaveCurrent();
    // From the Welcome screen there is nothing of another project to drop.
    else if (workspace()?.root !== info.root) closeAllTabs();
    setWorkspace(workspaceInfo(info));
    current = { ...info };
    if (runsLifecycle) void runProjectPhase("opened", current);
    return;
  }
  // The same project again: newer details (the event adds the VCS root and trust).
  current = { ...current, ...info };
  setWorkspace(workspaceInfo(current));
}

function lifecycleEvent(e: LifecycleEvent): void {
  if (e.phase === "closing") {
    // Closed by Rust (a Code With Me session ended, another project opens): its handlers
    // run now; `workspace()` follows with the next project or Close Project.
    if (current?.root === e.project.root) leaveCurrent();
  } else if (e.phase === "opened") {
    adopt(e.project);
  } else if (e.phase === "ready" && current?.root === e.project.root) {
    void runProjectPhase("ready", current);
  }
}

if (typeof window !== "undefined" && runsLifecycle) void projectLifecycleApi.onEvent(lifecycleEvent);

/**
 * `beforeClose` of the open project: the Rust subscribers' objections, then the UI handlers
 * (terminals with running commands ask "Terminate…?"). `false` keeps the project.
 */
export async function confirmCloseProject(nextRoot?: string): Promise<boolean> {
  const open = current;
  if (!open || open.root === nextRoot) return true;
  const objections = await projectLifecycleApi.beforeClose().catch(() => []);
  return confirmProjectClose(open, objections);
}

/** Rust's side of the last Close Project; opening waits for it. */
let backendClose: Promise<unknown> = Promise.resolve();

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
  // The open project may close first (running terminal commands ask).
  if (!(await confirmCloseProject(path))) return false;
  // Paths another backend serves (`\\wsl$\<distro>\…` opens in WSL).
  for (const handler of pathHandlers) if (await handler(path)) return true;
  if (options.askTrust !== false && trustPrompt && (await trustPrompt.beforeOpen(path)) === "cancel") return false;
  await backendClose;
  adopt(await workspaceApi.open(path));
  return true;
}

/** Open `path` on a host connected with `remoteApi.connect` as the project. */
export async function openRemoteProject(connectionId: string, path: string): Promise<void> {
  if (!(await confirmCloseProject())) return;
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
  const open = await workspaceApi.current();
  if (open) adopt(open);
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

/** Close the project without asking (the session ended, Don't Open); see {@link requestCloseWorkspace}. */
export function closeWorkspace(): void {
  leaveCurrent();
  setWorkspace(null);
  if (runsLifecycle) backendClose = projectLifecycleApi.close().catch(() => undefined);
}

/** File | Close Project: asks `beforeClose` first; `false` when the project stays open. */
export async function requestCloseWorkspace(): Promise<boolean> {
  if (!(await confirmCloseProject())) return false;
  closeWorkspace();
  return true;
}
