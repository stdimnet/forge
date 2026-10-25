/**
 * State of the Dev Containers and WSL UI: the "Create Dev Container" dialog with its build
 * log, and opening containers and WSL folders as projects.
 */
import { createSignal } from "solid-js";
import { devcontainerApi, wslApi, type BuildLogLine } from "@/ipc/devenv";
import type { RecentProject } from "@/ipc/workspace";
import { adoptRemoteWorkspace, workspace } from "@/state/workspace";
import { chooseRemoteFolder, setConnecting } from "@/features/remote/store";

// ---- the Create Dev Container dialog --------------------------------------------------------

export interface DevContainerDialogRequest {
  /** The local project folder. */
  folder: string;
  /** Preselected `devcontainer.json` (absolute). */
  configPath?: string;
  /** Remove the existing container first. */
  rebuild?: boolean;
}

export const [devContainerDialog, setDevContainerDialog] = createSignal<DevContainerDialogRequest | null>(null);

/** "Create Dev Container and Mount Sources…" for a local folder (default: the open project). */
export function createDevContainer(request: Partial<DevContainerDialogRequest> = {}): void {
  const folder = request.folder ?? (workspace()?.remote ? undefined : workspace()?.root);
  if (!folder) return;
  setDevContainerDialog({ ...request, folder });
}

export type BuildState = "idle" | "building" | "opening" | "done" | "failed";

export const [buildLog, setBuildLog] = createSignal<BuildLogLine[]>([]);

let listening = false;

/** Collect `devcontainer://log` lines while the dialog builds. */
export function listenToBuildLog(): void {
  if (listening) return;
  listening = true;
  void devcontainerApi.onLog((line) => setBuildLog((l) => [...l, line]));
}

// ---- opening ---------------------------------------------------------------------------------

/** Start the agent in a dev container and open its workspace folder. */
export async function openDevContainer(containerId: string, label: string): Promise<void> {
  setConnecting({ label, message: `Connecting to ${label}…` });
  try {
    const info = await devcontainerApi.open(containerId);
    setConnecting(null);
    await adoptRemoteWorkspace(info);
  } finally {
    setConnecting(null);
  }
}

/** Open a WSL distribution's folder: connect, choose the folder (unless given), open. */
export async function openWslProject(distro: string, path?: string): Promise<boolean> {
  setConnecting({ label: `WSL: ${distro}`, message: `Connecting to WSL: ${distro}…` });
  let connected: Awaited<ReturnType<typeof wslApi.connect>>;
  try {
    connected = await wslApi.connect(distro);
  } finally {
    setConnecting(null);
  }
  const folder =
    path ??
    (await chooseRemoteFolder(connected.connectionId, connected.label, connected.home, (p) =>
      wslApi.browse(connected.connectionId, p),
    ));
  if (!folder) {
    await wslApi.disconnect(connected.connectionId).catch(() => undefined);
    return false;
  }
  return adoptRemoteWorkspace(await wslApi.openProject(connected.connectionId, distro, folder));
}

/** Open a `\\wsl$\<distro>\…` / `\\wsl.localhost\…` path in its distribution. */
export async function openWslPath(path: string): Promise<boolean> {
  if (!/^(\\\\|\/\/)wsl(\$|\.localhost)[\\/]/i.test(path)) return false;
  const linux = await wslApi.mapPath(path).catch(() => null);
  if (!linux?.distro) return false;
  return openWslProject(linux.distro, linux.path);
}

/** Open a recent dev container or WSL project. Returns false for SSH projects. */
export async function openRecentDevEnv(project: RecentProject): Promise<boolean> {
  const kind = project.remote?.kind;
  if (!kind) return false;
  if (kind.type === "wsl") {
    await openWslProject(kind.distro, project.remote!.path);
    return true;
  }
  const list = await devcontainerApi.list();
  const container =
    list.containers.find((c) => c.containerId === kind.container) ??
    list.containers.find((c) => c.configFile === kind.config && c.localFolder === kind.source);
  if (container) {
    await openDevContainer(container.containerId, project.remote!.label);
  } else {
    // The container is gone: build it again from the sources.
    createDevContainer({ folder: kind.source, configPath: kind.config });
  }
  return true;
}
