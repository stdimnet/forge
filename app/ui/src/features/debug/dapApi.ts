/**
 * Typed client for Debug Adapter Protocol sessions (`app/src-tauri/src/commands/dap.rs`):
 * starting sessions, threads and the native views. Stepping, frames, variables, evaluate
 * and breakpoints use the shared `debugApi`.
 */
import { invoke } from "@/ipc";
import type { Breakpoint, Property, SessionInfo } from "./api";

/** A debug adapter of an enabled plugin. */
export interface AdapterInfo {
  type: string;
  label: string;
  /** File extensions whose line breakpoints it handles. */
  languages: string[];
  launch: boolean;
  attachProcess: boolean;
  coreDump: boolean;
  command?: string;
  launchSchema?: unknown;
  attachSchema?: unknown;
}

export type LaunchMode =
  | { mode: "launch" }
  | { mode: "attachProcess"; pid: number }
  | { mode: "attachServer"; host: string; port: number }
  | { mode: "coreDump"; core: string };

export type LaunchRequest = LaunchMode & {
  adapter: string;
  name?: string;
  program?: string;
  args?: string[];
  /** Program arguments as one command line (split like a shell). */
  arguments?: string;
  cwd?: string;
  env?: Record<string, string>;
  stopOnEntry?: boolean;
  /** Adapter-specific options merged into the launch/attach arguments. */
  configuration?: Record<string, unknown>;
};

export interface DapThread {
  id: number;
  name: string;
}

export interface MemoryRow {
  address: string;
  /** Two hex digits per byte, `??` when unreadable. */
  hex: string[];
  ascii: string;
}

export interface MemoryBlock {
  address: string;
  rows: MemoryRow[];
  unreadable: number;
}

export interface Instruction {
  address: string;
  bytes?: string;
  instruction: string;
  symbol?: string;
  file?: string;
  line?: number;
  /** The selected frame's instruction pointer. */
  current: boolean;
}

export interface DapModule {
  id: number | string;
  name: string;
  path?: string;
  version?: string;
  symbolStatus?: string;
  addressRange?: string;
}

export interface LoadedSource {
  name: string;
  path?: string;
  file?: string;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  commandLine: string;
  user?: string;
}

export const dapApi = {
  adapters: () => invoke<AdapterInfo[]>("dap_adapters"),
  /** Launch, attach (process or DAP server) or open a core dump. */
  start: (request: LaunchRequest) => invoke<SessionInfo>("dap_start", { request }),
  restart: (sessionId: string) => invoke<SessionInfo>("dap_restart", { sessionId }),
  threads: (sessionId: string) => invoke<DapThread[]>("dap_threads", { sessionId }),
  /** Set Execution Point. */
  goto: (sessionId: string, file: string, line: number) => invoke<void>("dap_goto", { sessionId, file, line }),
  registers: (sessionId: string, depth: number) => invoke<Property[]>("dap_registers", { sessionId, depth }),
  readMemory: (sessionId: string, memoryReference: string, count: number, offset = 0) =>
    invoke<MemoryBlock>("dap_read_memory", { sessionId, memoryReference, offset, count }),
  /** `data`: hex bytes, e.g. `de ad be ef`. */
  writeMemory: (sessionId: string, memoryReference: string, data: string, offset = 0) =>
    invoke<void>("dap_write_memory", { sessionId, memoryReference, offset, data }),
  disassemble: (sessionId: string, memoryReference: string, instructionOffset: number, count: number, current?: string) =>
    invoke<Instruction[]>("dap_disassemble", { sessionId, memoryReference, instructionOffset, count, current }),
  modules: (sessionId: string) => invoke<DapModule[]>("dap_modules", { sessionId }),
  loadedSources: (sessionId: string) => invoke<LoadedSource[]>("dap_loaded_sources", { sessionId }),
  /** Add a watchpoint on the variable `name` of `container`. */
  dataBreakpoint: (sessionId: string, container: number, name: string, accessType?: string) =>
    invoke<Breakpoint>("dap_data_breakpoint", { sessionId, container, name, accessType }),
  processes: (filter?: string) => invoke<ProcessInfo[]>("dap_processes", { filter }),
};
