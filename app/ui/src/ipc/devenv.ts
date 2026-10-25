/**
 * Dev Containers and WSL projects (`app/src-tauri/src/commands/devenv.rs`,
 * `crates/forge-devenv`). A container or a distribution runs the Forge agent over the stdio
 * of `docker exec` / `wsl.exe`; the project then opens like a remote project.
 */
import { invoke, listen } from "./index";
import type { BrowseResult } from "./remote";
import type { WorkspaceInfo } from "./workspace";

export type LifecycleCommand = string | string[] | Record<string, string | string[]>;

export type ContainerSource =
  | { kind: "image"; image: string }
  | { kind: "dockerfile"; dockerfile: string; context: string; args: Record<string, string>; target?: string }
  | { kind: "compose"; files: string[]; service: string; runServices: string[] };

/** `devcontainer.json`, resolved by Rust (absolute paths, variables replaced). */
export interface ResolvedDevContainer {
  id: string;
  name: string;
  configPath: string;
  localFolder: string;
  source: ContainerSource;
  workspaceFolder: string;
  workspaceMount?: string;
  mounts: string[];
  forwardPorts: { host?: string; port: number }[];
  onCreateCommand?: LifecycleCommand;
  postCreateCommand?: LifecycleCommand;
  postStartCommand?: LifecycleCommand;
  remoteUser?: string;
  containerUser?: string;
  containerEnv: Record<string, string>;
  remoteEnv: Record<string, string>;
  runArgs: string[];
  overrideCommand: boolean;
  features: { id: string; options: unknown }[];
  warnings: string[];
}

export interface DevContainerConfigView {
  /** Absolute path of `devcontainer.json`. */
  path: string;
  /** `.devcontainer/devcontainer.json` */
  rel: string;
  config?: ResolvedDevContainer;
  /** "Image php:8.3", "Dockerfile …", "Docker Compose service app". */
  source?: string;
  error?: string;
}

/** A dev container known to Docker. */
export interface DevContainerInstance {
  containerId: string;
  containerName: string;
  image: string;
  /** `running`, `exited`, `created`, … */
  state: string;
  /** `Up 2 hours` */
  status: string;
  created: string;
  id: string;
  name: string;
  workspaceFolder: string;
  remoteUser?: string;
  localFolder: string;
  configFile: string;
  composeProject?: string;
}

export interface DevContainerList {
  /** The Docker CLI answered. */
  available: boolean;
  error?: string;
  containers: DevContainerInstance[];
}

/** A line of the build log (`devcontainer://log`). */
export type BuildLogLine =
  | { kind: "step"; text: string }
  | { kind: "command"; text: string }
  | { kind: "output"; stream: "stdout" | "stderr"; text: string }
  | { kind: "warning"; text: string };

export interface UpOutcome {
  containerId: string;
  /** A new container was created (not just started). */
  created: boolean;
}

export type DevContainerAction = "start" | "stop" | "remove";

export interface WslDistro {
  name: string;
  /** `Running`, `Stopped`, … */
  state: string;
  version: number;
  default: boolean;
}

export interface WslInfo {
  /** WSL is installed (Windows only). */
  available: boolean;
  error?: string;
  distros: WslDistro[];
}

export interface LinuxPath {
  /** The distribution with the file (absent for Windows drives under `/mnt`). */
  distro?: string;
  path: string;
}

export interface WslConnected {
  connectionId: string;
  label: string;
  home: string;
}

export const DEVCONTAINER_LOG_EVENT = "devcontainer://log";

export const devcontainerApi = {
  /** The `devcontainer.json` files of a local folder (default: the open project). */
  configs: (folder?: string) => invoke<DevContainerConfigView[]>("devcontainer_configs", { folder }),
  /** Line of the gutter icon in a `devcontainer.json` (`null`: none). */
  gutter: (path: string, content: string) => invoke<number | null>("devcontainer_gutter", { path, content }),
  list: () => invoke<DevContainerList>("devcontainer_list"),
  /** Build or start the container; refused for untrusted folders. */
  up: (folder: string, configPath: string, rebuild = false) =>
    invoke<UpOutcome>("devcontainer_up", { folder, configPath, rebuild }),
  cancel: () => invoke<void>("devcontainer_cancel"),
  action: (containerId: string, action: DevContainerAction) =>
    invoke<void>("devcontainer_action", { containerId, action }),
  /** Start the agent in the container and open its workspace folder as the project. */
  open: (containerId: string) => invoke<WorkspaceInfo>("devcontainer_open", { containerId }),
  onLog: (cb: (line: BuildLogLine) => void) => listen<BuildLogLine>(DEVCONTAINER_LOG_EVENT, cb),
};

export const wslApi = {
  distributions: () => invoke<WslInfo>("wsl_distributions"),
  mapPath: (path: string) => invoke<LinuxPath | null>("wsl_map_path", { path }),
  windowsPath: (distro: string, path: string) => invoke<string>("wsl_windows_path", { distro, path }),
  connect: (distro: string) => invoke<WslConnected>("wsl_connect", { distro }),
  browse: (connectionId: string, path: string) => invoke<BrowseResult>("wsl_browse", { connectionId, path }),
  openProject: (connectionId: string, distro: string, path: string) =>
    invoke<WorkspaceInfo>("wsl_open_project", { connectionId, distro, path }),
  disconnect: (connectionId: string) => invoke<void>("wsl_disconnect", { connectionId }),
};
