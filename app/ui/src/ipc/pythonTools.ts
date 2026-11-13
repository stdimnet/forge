/**
 * PyForge's run tools (app/src-tauri/src/commands/python_tools.rs, crate
 * packs/pyforge/testing): pytest / unittest / tox detection and tox environments, debugpy
 * (status and where a debug run listens), the Python Console processes and the cProfile
 * profiler snapshots. Test runs, debug runs and Profile go through `run_start`.
 */
import { invoke, listen } from "./index";
import type { ConsoleDelta, RunConfiguration } from "./run";

export interface OutputPathMapping {
  remote: string;
  local: string;
}

export interface ProjectTesting {
  pytest: boolean;
  tox: boolean;
  reasons: string[];
}

export interface ToxEnv {
  name: string;
  /** In tox's default `env_list`. */
  default: boolean;
  description?: string;
  file: string;
  line?: number;
}

export interface DebugpyStatus {
  installed: boolean;
  version?: string;
  interpreter: string;
}

export interface DebugPlan {
  listenHost: string;
  connectHost: string;
  port: number;
  pathMappings: { localRoot: string; remoteRoot: string }[];
  justMyCode: boolean;
  subProcess: boolean;
}

export interface ConsoleStarted {
  id: string;
  commandLine: string;
  interpreter: string;
}

/** A variable of the console's namespace (or a member of one). */
export interface ConsoleVariable {
  name: string;
  type: string;
  value: string;
  size?: number | null;
  /** Expression that reads it (`orders[0].total`), for its members. */
  expr: string;
  children: boolean;
}

/** Replies of Forge's console helper (`forge_console.py`). */
export type ConsoleMessage =
  | { type: "ready"; banner: string; prompt: string; more: string; ipython: boolean }
  | { type: "more"; prompt: string }
  | { type: "done"; prompt: string }
  | { type: "vars"; vars: ConsoleVariable[] }
  | { type: "children"; expr: string; vars: ConsoleVariable[] }
  | { type: "input"; prompt: string };

export type ConsoleEvent =
  | ({ type: "console" } & ConsoleDelta)
  | { type: "message"; message: ConsoleMessage }
  | { type: "exited"; code: number | null };

export interface ProfileInfo {
  id: string;
  name: string;
  at: number;
  path: string;
  mappings?: OutputPathMapping[];
}

export interface HotSpot {
  function: number;
  name: string;
  file: string;
  line: number;
  calls: number;
  primitiveCalls: number;
  ownMs: number;
  totalMs: number;
  ownPercent: number;
  totalPercent: number;
}

export interface CallNode {
  function: number;
  name: string;
  file: string;
  line: number;
  calls: number;
  ownMs: number;
  totalMs: number;
  totalPercent: number;
  children?: CallNode[];
}

export interface ProfileView {
  totalMs: number;
  functions: number;
  calls: number;
  hotSpots: HotSpot[];
  callTree: CallNode[];
}

export interface ProfileSnapshot {
  info: ProfileInfo;
  view: ProfileView;
}

export const pythonToolsApi = {
  testingDetect: () => invoke<ProjectTesting>("python_testing_detect"),
  toxEnvironments: () => invoke<ToxEnv[]>("python_tox_environments"),
  debugpyStatus: (interpreter?: string) => invoke<DebugpyStatus>("python_debugpy_status", { interpreter: interpreter || null }),
  debugPlan: (configuration: RunConfiguration) => invoke<DebugPlan>("python_debug_plan", { configuration }),
  consoleStart: (id?: string) => invoke<ConsoleStarted>("python_console_start", { id: id ?? null }),
  consoleSend: (id: string, op: string, fields?: Record<string, unknown>, echo?: string) =>
    invoke<void>("python_console_send", { id, op, fields: fields ?? null, echo: echo ?? null }),
  consoleStop: (id: string) => invoke<void>("python_console_stop", { id }),
  onConsole: (id: string, cb: (e: ConsoleEvent) => void) => listen<ConsoleEvent>(`python-console://${id}`, cb),
  consoleHistory: () => invoke<string[]>("python_console_history"),
  consoleHistoryAdd: (entry: string) => invoke<string[]>("python_console_history_add", { entry }),
  profiles: () => invoke<ProfileInfo[]>("python_profiles"),
  profileView: (id: string) => invoke<ProfileSnapshot>("python_profile_view", { id }),
  profileOpen: (path: string) => invoke<ProfileInfo>("python_profile_open", { path }),
  profileDelete: (id: string) => invoke<void>("python_profile_delete", { id }),
};
