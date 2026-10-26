/**
 * Typed client for the debugger (`app/src-tauri/src/commands/debug.rs`). The same commands
 * drive Xdebug sessions (`xdebug-<n>`) and Debug Adapter Protocol sessions (`dap-<n>`, see
 * `dapApi.ts` for what only DAP sessions offer).
 */
import { invoke, listen } from "@/ipc";

export type SessionStatus = "initializing" | "awaitingAcceptance" | "running" | "paused" | "stopped";
export type BreakReason = "breakpoint" | "step" | "firstLine" | "exception" | "pause";
export type StepKind = "run" | "stepInto" | "forceStepInto" | "stepOver" | "stepOut" | "stepIntoInstruction" | "stepOverInstruction";

/** An exception filter of a debug adapter ("C++: on throw"). */
export interface ExceptionFilter {
  filter: string;
  label: string;
  description?: string;
  default: boolean;
  supportsCondition: boolean;
}

/** Optional features of a DAP session (its adapter's capabilities). */
export interface SessionFeatures {
  threads: boolean;
  readMemory: boolean;
  writeMemory: boolean;
  disassemble: boolean;
  registers: boolean;
  dataBreakpoints: boolean;
  functionBreakpoints: boolean;
  conditionalBreakpoints: boolean;
  hitConditionalBreakpoints: boolean;
  logPoints: boolean;
  instructionStepping: boolean;
  goto: boolean;
  restart: boolean;
  completions: boolean;
  setVariable: boolean;
  modules: boolean;
  loadedSources: boolean;
  /** Attached to a running program: Stop detaches. */
  attached: boolean;
  exceptionFilters: ExceptionFilter[];
}

export interface Location {
  /** Project relative file; null when the server file is not mapped. */
  file: string | null;
  fileUri: string;
  line: number;
}

export interface SessionInfo {
  id: string;
  ideKey: string;
  language: string;
  engine: string;
  fileUri: string;
  file: string | null;
  peer: string;
  serverName: string;
  host?: string;
  port?: number;
  serverId: string | null;
  status: SessionStatus;
  location: Location | null;
  reason: BreakReason | null;
  exception: { name: string; message: string | null } | null;
  /** Debug adapter type of a DAP session (`lldb`); absent for Xdebug. */
  adapter?: string;
  features?: SessionFeatures;
  /** The thread that stopped (DAP). */
  threadId?: number;
  /** The session that started this one (DAP child sessions). */
  parentId?: string;
}

export interface Frame {
  level: number;
  file: string | null;
  fileUri: string;
  line: number;
  where: string;
  /** DAP frame id. */
  id?: number;
  /** DAP: instruction pointer (Disassembly). */
  address?: string;
}

export interface Property {
  name: string;
  fullname: string;
  type: string;
  classname?: string;
  facet?: string;
  value?: string;
  size?: number;
  hasChildren: boolean;
  numChildren: number;
  page: number;
  pageSize: number;
  children: Property[];
  /** DAP `variablesReference` of the children. */
  reference?: number;
  /** DAP: reference of the scope or variable holding this one (Set Value, watchpoints). */
  container?: number;
  /** DAP: memory location of the value (View Memory). */
  memoryReference?: string;
}

export type BreakpointKind = "line" | "exception" | "function" | "data";

/** Fields of function breakpoints, watchpoints and adapter exception filters (DAP). */
interface DapBreakpointFields {
  /** Function name (function breakpoints). */
  function?: string;
  /** The adapter's data id (data breakpoints). */
  dataId?: string;
  /** `write`, `read` or `readWrite` (data breakpoints). */
  accessType?: string;
  /** What a watchpoint watches, or an exception filter's label. */
  description?: string;
  /** Debug adapter type of an exception filter or watchpoint. */
  adapter?: string;
}

export interface Breakpoint extends DapBreakpointFields {
  id: number;
  kind: BreakpointKind;
  file?: string;
  line?: number;
  exception?: string;
  condition?: string;
  enabled: boolean;
  /** "Suspend execution"; false makes a logpoint. Missing = true. */
  suspend?: boolean;
  /** "Log: 'Breakpoint hit' message". */
  logMessage?: boolean;
  /** "Log: Stack trace". */
  logStack?: boolean;
  /** "Evaluate and log". */
  logExpression?: string;
  /** "Remove once hit". */
  temporary?: boolean;
  /** "Pass count": suspend from the n-th hit on. */
  passCount?: number;
}

export interface BreakpointSpec extends DapBreakpointFields {
  kind?: BreakpointKind;
  file?: string;
  line?: number;
  exception?: string;
  condition?: string;
  enabled?: boolean;
  suspend?: boolean;
  logMessage?: boolean;
  logStack?: boolean;
  logExpression?: string;
  temporary?: boolean;
  passCount?: number;
}

/** A completion item of the debugger console / Evaluate dialog. */
export interface Completion {
  label: string;
  /** Replaces the typed prefix. */
  insert: string;
  detail: string;
}

export type CheckStatus = "ok" | "warning" | "error" | "info";

export interface ValidationCheck {
  title: string;
  value: string;
  status: CheckStatus;
  hint?: string;
}

/** Result of Run | Web Server Debug Validation. */
export interface ValidationReport {
  checks: ValidationCheck[];
  ok: boolean;
  environment: { phpVersion: string; sapi: string; phpIni: string; xdebug: string | null; serverName?: string | null };
}

export interface WebPageStarted {
  runId: string;
  commandLine: string;
  url: string;
}

export interface BreakpointsSnapshot {
  breakpoints: Breakpoint[];
  muted: boolean;
}

export interface PathMapping {
  /** Project relative path ("" = project root). */
  local: string;
  /** Absolute path on the server. */
  remote: string;
}

export interface ServerConfig {
  id: string;
  name: string;
  host: string;
  port: number;
  debugger: string;
  usePathMappings: boolean;
  mappings: PathMapping[];
}

export interface DebugStatus {
  listening: boolean;
  /** The port Xdebug connects to (on the remote host for a remote project). */
  port: number;
  sessions: SessionInfo[];
  /** Remote project: the host whose debug port is forwarded over SSH (`deploy@shop.example.com`). */
  via?: string;
}

export interface ContextName {
  id: number;
  name: string;
}

export type DebugEvent =
  | { type: "listening"; listening: boolean; port: number; error?: string }
  | { type: "session"; session: SessionInfo }
  | { type: "incoming"; session: SessionInfo }
  | { type: "ended"; sessionId: string }
  /** `stream`: stdout, stderr or log (logpoints). */
  | { type: "output"; sessionId: string; stream: string; text: string }
  | { type: "breakpoints"; snapshot: BreakpointsSnapshot }
  /** Views showing `areas` (`memory`, `threads`, `modules`, `variables`) reload. */
  | { type: "invalidated"; sessionId: string; areas: string[] }
  | { type: "error"; sessionId: string | null; message: string };

const CHANNELS = ["listening", "session", "incoming", "ended", "output", "breakpoints", "invalidated", "error"] as const;

export const debugApi = {
  status: () => invoke<DebugStatus>("debug_status"),
  listenStart: () => invoke<number>("debug_listen_start"),
  listenStop: () => invoke<void>("debug_listen_stop"),
  sessions: () => invoke<SessionInfo[]>("debug_sessions"),
  acceptConnection: (sessionId: string, serverConfig: ServerConfig) =>
    invoke<void>("debug_accept_connection", { sessionId, serverConfig }),
  ignoreConnection: (sessionId: string) => invoke<void>("debug_ignore_connection", { sessionId }),
  step: (sessionId: string, kind: StepKind) => invoke<void>("debug_step", { sessionId, kind }),
  pause: (sessionId: string) => invoke<void>("debug_pause", { sessionId }),
  stop: (sessionId: string) => invoke<void>("debug_stop", { sessionId }),
  detach: (sessionId: string) => invoke<void>("debug_detach", { sessionId }),
  runToCursor: (sessionId: string, file: string, line: number) =>
    invoke<void>("debug_run_to_cursor", { sessionId, file, line }),
  /** Frames; DAP sessions show `threadId` (default: the current thread). */
  stack: (sessionId: string, threadId?: number) => invoke<Frame[]>("debug_stack", { sessionId, threadId }),
  contexts: (sessionId: string, depth: number) => invoke<ContextName[]>("debug_contexts", { sessionId, depth }),
  variables: (sessionId: string, depth: number, context = 0) =>
    invoke<Property[]>("debug_variables", { sessionId, depth, context }),
  /** Children of a variable: DBGp by `fullname` and page, DAP by `reference`. */
  property: (sessionId: string, fullname: string, depth: number, context = 0, page = 0, reference?: number) =>
    invoke<Property>("debug_property", { sessionId, fullname, depth, context, page, reference }),
  /** `context` (DAP): `watch`, `repl` (console, Evaluate) or `hover`. */
  eval: (sessionId: string, expression: string, depth = 0, context?: "watch" | "repl" | "hover" | "clipboard") =>
    invoke<Property>("debug_eval", { sessionId, expression, depth, context }),
  /** Set Value; DAP variables are addressed by `container` and `name`. */
  setValue: (sessionId: string, fullname: string, value: string, depth = 0, container?: number, name?: string) =>
    invoke<void>("debug_set_value", { sessionId, fullname, value, depth, container, name }),
  breakpoints: () => invoke<BreakpointsSnapshot>("debug_breakpoints"),
  addBreakpoint: (spec: BreakpointSpec) => invoke<Breakpoint>("debug_breakpoint_add", { spec }),
  updateBreakpoint: (breakpoint: Breakpoint) => invoke<Breakpoint>("debug_breakpoint_update", { breakpoint }),
  removeBreakpoint: (id: number) => invoke<void>("debug_breakpoint_remove", { id }),
  toggleBreakpoint: (file: string, line: number) =>
    invoke<Breakpoint | null>("debug_breakpoint_toggle", { file, line }),
  mute: (muted: boolean) => invoke<void>("debug_breakpoints_mute", { muted }),
  watches: () => invoke<string[]>("debug_watches"),
  setWatches: (watches: string[]) => invoke<void>("debug_watches_set", { watches }),
  servers: () => invoke<ServerConfig[]>("debug_servers"),
  saveServers: (servers: ServerConfig[]) => invoke<void>("debug_servers_save", { servers }),
  /** Completion of `text` (caret at its end) in a frame: variables after `$`, members after `->`. */
  complete: (sessionId: string, text: string, depth = 0) => invoke<Completion[]>("debug_complete", { sessionId, text, depth }),
  /** PHP Web Page: report the URL on `run://<runId>` and return it for the browser. */
  webPageStart: (runId: string, serverId: string | undefined, startUrl: string, debug: boolean) =>
    invoke<WebPageStarted>("debug_web_page_start", { runId, serverId, startUrl, debug }),
  /** The URL of `startUrl` on a server (with `XDEBUG_SESSION_START` when debugging). */
  pageUrl: (serverId: string | undefined, startUrl: string, debug = false) =>
    invoke<string>("debug_page_url", { serverId, startUrl, debug }),
  /** Validate a CLI interpreter (by name, empty = the project's) from `php -i`. */
  validateCli: (interpreter?: string) => invoke<ValidationReport>("debug_validate_cli", { interpreter }),
  /** Place the validation script in `documentRoot`, request it under `url`, delete it. */
  validateWeb: (documentRoot: string, url: string) => invoke<ValidationReport>("debug_validate_web", { documentRoot, url }),
  /** Env vars that make a PHP process connect to the debugger (for Debug run configurations). */
  launchEnv: (serverName?: string) => invoke<Record<string, string>>("debug_launch_env", { serverName }),
  /** Subscribe to every `debug://*` event. */
  onEvent: async (cb: (e: DebugEvent) => void): Promise<() => void> => {
    const offs = await Promise.all(CHANNELS.map((c) => listen<DebugEvent>(`debug://${c}`, cb)));
    return () => offs.forEach((off) => off());
  },
};
