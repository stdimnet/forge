/**
 * DotNetForge for game engines (`app/src-tauri/src/commands/gamedev.rs`, logic in
 * `packs/dotnetforge/gamedev`): Unity (editors, the Forge editor bridge, project files,
 * processes for debugging) and Godot (the project, the executable, the editor's servers,
 * debugging). Everything is computed in Rust.
 */
import { invoke, listen } from "./index";
import type { CommandLine, RunConfiguration } from "./run";
import type { LaunchRequest } from "@/features/debug/dapApi";

// ---- Unity ----------------------------------------------------------------------------------

export interface UnityPackage {
  name: string;
  version: string;
}

export interface BridgeInstall {
  /** `embedded`, `manifest` or `none`. */
  kind: string;
  version?: string | null;
  update: boolean;
}

export interface UnityProject {
  version: string;
  revision?: string | null;
  packages: UnityPackage[];
  idePackages: string[];
  testFramework?: string | null;
  bridge: BridgeInstall;
  solutions: string[];
  csprojects: string[];
}

export interface UnityEditor {
  version: string;
  path: string;
  /** `hub`, `hubManual` or `custom`. */
  source: string;
}

export type PlayMode = "edit" | "playing" | "paused";

export interface Welcome {
  protocol: number;
  unityVersion: string;
  projectPath: string;
  pid: number;
  playMode: PlayMode;
  compiling: boolean;
}

export interface UnityStatus {
  project: UnityProject | null;
  editors: UnityEditor[];
  editor: UnityEditor | null;
  exact: boolean;
  editorRunning: number | null;
  bridgeAvailable: boolean;
  connected: boolean;
  welcome: Welcome | null;
  bundledBridge: string;
  remote: boolean;
}

export type LogLevel = "log" | "warning" | "error" | "exception" | "assert";

export interface LogEntry {
  level: LogLevel;
  message: string;
  stackTrace?: string;
  time?: string | null;
  /** 1-based line. */
  location?: { path: string; line: number; column?: number | null } | null;
}

export type BridgeEvent =
  | { type: "state"; playMode: PlayMode; compiling: boolean }
  | { type: "log"; entry: LogEntry }
  | { type: "openFile"; path: string; line: number; column: number }
  | { type: "disconnected"; reason: string };

export type BridgeCommand = "play" | "pause" | "resume" | "stop" | "step" | "refresh" | "syncSolution";

export interface Ack {
  ok: boolean;
  message?: string | null;
}

export interface UnityProcess {
  pid: number;
  name: string;
  project?: string | null;
  thisProject: boolean;
  debugPort: number;
}

export const unityApi = {
  status: () => invoke<UnityStatus>("unity_status"),
  installBridge: () => invoke<BridgeInstall>("unity_install_bridge"),
  connect: () => invoke<Welcome>("unity_bridge_connect"),
  disconnect: () => invoke<void>("unity_bridge_disconnect"),
  send: (command: BridgeCommand) => invoke<Ack>("unity_bridge_command", { command }),
  /** `openEditor` or `syncSolution`: a command line to run. */
  command: (action: "openEditor" | "syncSolution") => invoke<CommandLine>("unity_command", { action }),
  processes: () => invoke<UnityProcess[]>("unity_processes"),
  attachRequest: (port: number, host?: string, name?: string) => invoke<LaunchRequest>("unity_attach_request", { port, host, name }),
  onBridge: (cb: (e: BridgeEvent) => void) => listen<BridgeEvent>("unity://bridge", cb),
};

// ---- Godot ----------------------------------------------------------------------------------

export interface GodotProject {
  name: string;
  mainScene?: string | null;
  version?: string | null;
  features: string[];
  configVersion: number;
  csharp: boolean;
  assemblyName?: string | null;
  autoloads: { name: string; path: string }[];
}

export interface GodotExecutable {
  path: string;
  /** `setting`, `environment`, `path` or `folder`. */
  source: string;
  dotnet: boolean;
}

export interface GodotStatus {
  project: GodotProject | null;
  scenes: string[];
  godot: GodotExecutable | null;
  version: { version: string; status: string; dotnet: boolean } | null;
  host: string;
  lspPort: number;
  dapPort: number;
  lspListening: boolean;
  dapListening: boolean;
  remote: boolean;
}

export const godotApi = {
  status: () => invoke<GodotStatus>("godot_status"),
  command: (action: "openEditor") => invoke<CommandLine>("godot_command", { action }),
  debugRequest: (configuration: RunConfiguration) => invoke<LaunchRequest>("godot_debug_request", { configuration }),
};
