/**
 * Browser-mode JavaScript debugger: js-debug's status (installed), the download (canned
 * steps), the browsers of this "machine", the launch request of a configuration (a stand-in
 * for `webforge_js_debug::request`), and a simulated js-debug session of the Node.js demo
 * (`?project=node`): the launcher session plus the child session js-debug asks for, which
 * stops at breakpoints in `src/orders/pricing.ts` (TypeScript, as through a source map) and
 * steps back into `src/server.ts`. Other sessions go to the C program of `dapMock.ts`.
 * Everything stays in memory.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { Breakpoint, BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { AdapterInfo, LaunchRequest } from "@/features/debug/dapApi";
import type { Browser, JsDebugStatus } from "@/ipc/jsDebug";
import type { RunConfiguration } from "@/ipc/run";

const ROOT = "/home/anna/Projects/orders-api";
const PRICING = "src/orders/pricing.ts";
const SERVER = "src/server.ts";
const JS_LANGUAGES = ["js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx", "vue", "svelte", "astro"];

const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const call = <T>(cmd: string, args: Record<string, unknown> = {}) => mockHandler(cmd)!(args) as T;

const STATUS: JsDebugStatus = {
  server: "/home/anna/.local/share/forge/debug-adapters/js-debug/1.140.0/src/dapDebugServer.js",
  custom: false,
  release: {
    version: "1.140.0",
    url: "https://github.com/microsoft/vscode-js-debug/releases/download/v1.140.0/js-debug-dap-v1.140.0.tar.gz",
    sha256: "27dab92937ec1ab35821ae955aac867544fe06a1b6307229049f2d789af10968",
    size: 1249709,
  },
  node: "/usr/bin/node",
};

const BROWSERS: Browser[] = [
  { kind: "chrome", name: "Google Chrome", path: "/usr/bin/google-chrome" },
  { kind: "chromium", name: "Chromium", path: "/usr/bin/chromium" },
];

const ADAPTERS: AdapterInfo[] = [
  { type: "pwa-node", label: "Node.js (js-debug)", languages: JS_LANGUAGES, launch: true, attachProcess: true, attachPort: true, coreDump: false, command: "/usr/bin/node" },
  { type: "pwa-chrome", label: "Chrome (js-debug)", languages: JS_LANGUAGES, launch: true, attachProcess: false, attachPort: true, coreDump: false, command: "/usr/bin/node" },
  { type: "pwa-msedge", label: "Edge (js-debug)", languages: JS_LANGUAGES, launch: true, attachProcess: false, attachPort: true, coreDump: false, command: "/usr/bin/node" },
];

registerMock("js_debug_status", () => STATUS);
registerMock("js_debug_browsers", () => BROWSERS);
registerMock("js_debug_install", async () => {
  for (const message of [`Using Node.js at ${STATUS.node}.`, `Downloading js-debug ${STATUS.release.version} from ${STATUS.release.url}…`, `Verified SHA-256 ${STATUS.release.sha256} of js-debug ${STATUS.release.version}.`])
    emitMock("js-debug://install", { message });
  return STATUS;
});

/** What `webforge_js_debug::request` answers, for the demo's configuration types. */
registerMock("js_debug_request", ({ configuration }) => {
  const cfg = configuration as RunConfiguration;
  const o = cfg.options ?? {};
  switch (cfg.type) {
    case "javascript-debug":
      return { mode: "launch", adapter: o.browser === "edge" ? "pwa-msedge" : "pwa-chrome", name: cfg.name, program: o.url, configuration: { runtimeExecutable: BROWSERS[0].path, userDataDir: true } };
    case "js-attach":
      return { mode: "attachPort", adapter: o.debugger === "chrome" ? "pwa-chrome" : o.debugger === "edge" ? "pwa-msedge" : "pwa-node", name: cfg.name, host: o.host || "localhost", port: Number(o.port || 9229) };
    case "npm":
      return { mode: "launch", adapter: "pwa-node", name: cfg.name, configuration: { runtimeExecutable: o.packageManager || "npm", runtimeArgs: ["run", o.script ?? ""] } };
    case "node":
      return { mode: "launch", adapter: "pwa-node", name: cfg.name, program: o.file, arguments: cfg.arguments, env: cfg.env, configuration: { runtimeArgs: (cfg.interpreterOptions ?? "").split(/\s+/).filter(Boolean) } };
    default:
      return { mode: "launch", adapter: "pwa-node", name: cfg.name, program: `node_modules/${cfg.type}/bin/${cfg.type}.js`, args: ["--runInBand"] };
  }
});

// ---- the simulated program ------------------------------------------------------------------

interface JsStep {
  /** [function, file, line], top frame first; a null file is Node.js' own code. */
  frames: [string, string | null, number][];
  locals: Record<string, string>;
}

const ORDER = '{id: 1042, items: Array(2), voucher: {…}}';

/** One request to `/orders/1042/total` through `orderTotal`. */
const TRACE: JsStep[] = [
  { frames: [["<anonymous>", SERVER, 14], ["emit", null, 518], ["parserOnIncoming", null, 1141]], locals: { req: "IncomingMessage", res: "ServerResponse", url: "URL", match: "Array(2)" } },
  { frames: [["<anonymous>", SERVER, 19], ["emit", null, 518], ["parserOnIncoming", null, 1141]], locals: { req: "IncomingMessage", res: "ServerResponse", url: "URL", match: "Array(2)", order: ORDER } },
  { frames: [["orderTotal", PRICING, 10], ["<anonymous>", SERVER, 19], ["emit", null, 518]], locals: { order: ORDER, subtotal: "undefined", discount: "undefined", tax: "undefined" } },
  { frames: [["orderTotal", PRICING, 11], ["<anonymous>", SERVER, 19], ["emit", null, 518]], locals: { order: ORDER, subtotal: "59.97", discount: "undefined", tax: "undefined" } },
  { frames: [["orderTotal", PRICING, 12], ["<anonymous>", SERVER, 19], ["emit", null, 518]], locals: { order: ORDER, subtotal: "59.97", discount: "5.997", tax: "undefined" } },
  { frames: [["orderTotal", PRICING, 13], ["<anonymous>", SERVER, 19], ["emit", null, 518]], locals: { order: ORDER, subtotal: "59.97", discount: "5.997", tax: "10.25487" } },
  { frames: [["<anonymous>", SERVER, 20], ["emit", null, 518], ["parserOnIncoming", null, 1141]], locals: { req: "IncomingMessage", res: "ServerResponse", url: "URL", match: "Array(2)", order: ORDER, total: "64.23" } },
];

const ORDER_CHILDREN: [string, string][] = [
  ["id", "1042"],
  ["items", "(2) [{…}, {…}]"],
  ["voucher", '{code: "SPRING10", percent: 10}'],
];

interface JsSession {
  info: SessionInfo;
  pc: number;
}

const sessions = new Map<string, JsSession>();
let seq = 900;

const features = (attached: boolean): SessionInfo["features"] => ({
  threads: true,
  readMemory: false,
  writeMemory: false,
  disassemble: false,
  registers: false,
  dataBreakpoints: false,
  functionBreakpoints: false,
  conditionalBreakpoints: true,
  hitConditionalBreakpoints: true,
  logPoints: true,
  instructionStepping: false,
  goto: false,
  restart: true,
  completions: true,
  setVariable: true,
  modules: false,
  loadedSources: true,
  attached,
  exceptionFilters: [
    { filter: "all", label: "Caught Exceptions", default: false, supportsCondition: true },
    { filter: "uncaught", label: "Uncaught Exceptions", default: false, supportsCondition: true },
  ],
});

const top = (s: JsSession) => TRACE[s.pc].frames[0];

function output(id: string, stream: string, text: string) {
  emit({ type: "output", sessionId: id, stream, text });
}

function breakpointAt(s: JsSession): Breakpoint | undefined {
  const snap = call<BreakpointsSnapshot>("debug_breakpoints");
  if (snap.muted) return undefined;
  const [, file, line] = top(s);
  return snap.breakpoints.find((b) => b.enabled && b.kind === "line" && b.file === file && b.line === line);
}

function stopped(s: JsSession, reason: SessionInfo["reason"]) {
  const [, file, line] = top(s);
  s.info = { ...s.info, status: "paused", reason, threadId: 1, location: { file, fileUri: `${ROOT}/${file}`, line } };
  emit({ type: "session", session: s.info });
}

/** Run until a breakpoint; at the end of the request the server waits for the next one. */
function run(s: JsSession, mode: "continue" | "next" | "stepIn" | "stepOut") {
  const depth = (x: number) => (TRACE[x].frames[0][0] === "orderTotal" ? 2 : 1);
  const start = depth(s.pc);
  for (;;) {
    if (s.pc + 1 >= TRACE.length) {
      output(s.info.id, "stdout", "GET /orders/1042/total 200 (64.23)\n");
      s.pc = 0;
      s.info = { ...s.info, status: "running", location: null, reason: null };
      emit({ type: "session", session: s.info });
      return;
    }
    s.pc += 1;
    if (breakpointAt(s)) return stopped(s, "breakpoint");
    const d = depth(s.pc);
    if (mode === "stepIn" || (mode === "next" && d <= start) || (mode === "stepOut" && d < start)) return stopped(s, "step");
  }
}

function resume(s: JsSession, fn: () => void) {
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => sessions.has(s.info.id) && fn(), 120);
}

function sessionInfo(id: string, request: LaunchRequest, title: string, parentId?: string): SessionInfo {
  const adapter = ADAPTERS.find((a) => a.type === request.adapter) ?? ADAPTERS[0];
  const program = request.program ?? "";
  return {
    id,
    ideKey: "",
    language: "ts",
    engine: adapter.label,
    fileUri: program.startsWith("/") || program.includes("://") ? program : `${ROOT}/${program}`,
    file: program.includes("://") ? null : program || null,
    peer: "127.0.0.1:41783",
    serverName: title,
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: adapter.type,
    features: features(request.mode !== "launch"),
    parentId,
  };
}

/** A js-debug session: the launcher, then the child that runs (and stops) the program. */
function start(request: LaunchRequest): SessionInfo {
  const parentId = `dap-${++seq}`;
  const title = request.name ?? request.program?.split("/").pop() ?? "Node.js";
  const parent: JsSession = { info: sessionInfo(parentId, request, title), pc: 0 };
  sessions.set(parentId, parent);
  emit({ type: "session", session: parent.info });
  const runtimeArgs = ((request.configuration as Record<string, unknown> | undefined)?.runtimeArgs as string[] | undefined) ?? [];
  if (request.mode === "launch" && request.adapter === "pwa-node")
    output(parentId, "log", `/usr/bin/node ${[...runtimeArgs, request.program ?? ""].join(" ")}\n`);
  setTimeout(() => {
    if (!sessions.has(parentId)) return;
    const childId = `dap-${++seq}`;
    const child: JsSession = { info: sessionInfo(childId, request, `${title} [48211]`, parentId), pc: 0 };
    sessions.set(childId, child);
    emit({ type: "session", session: child.info });
    output(parentId, "stderr", "Debugger attached.\n");
    output(parentId, "stdout", "Orders API listening on http://localhost:3000\n");
    // The first request arrives: run to the first breakpoint of the trace.
    setTimeout(() => {
      if (!sessions.has(childId)) return;
      if (breakpointAt(child)) stopped(child, "breakpoint");
      else run(child, "continue");
    }, 400);
  }, 250);
  return parent.info;
}

function end(id: string) {
  for (const s of [...sessions.values()].filter((x) => x.info.id === id || x.info.parentId === id)) {
    sessions.delete(s.info.id);
    emit({ type: "ended", sessionId: s.info.id });
  }
}

const prop = (name: string, value: string, reference?: number): Property => ({
  name,
  fullname: name,
  type: "dap",
  classname: /^-?\d/.test(value) ? "number" : value === "undefined" ? "undefined" : value.startsWith("{") || value.startsWith("(") ? "Object" : value,
  value,
  hasChildren: reference !== undefined,
  numChildren: reference !== undefined ? 3 : 0,
  page: 0,
  pageSize: 0,
  children: [],
  reference,
});

// ---- handlers -------------------------------------------------------------------------------

const ORDER_REF = 7001;

const own = (args: Record<string, unknown>) => sessions.get(String(args.sessionId ?? ""));

/** Answer `cmd` for js-debug sessions, else hand it to the previous mock. */
function route(cmd: string, fn: (s: JsSession, args: Record<string, unknown>) => unknown) {
  const previous = mockHandler(cmd);
  registerMock(cmd, (args) => {
    const s = own(args);
    if (s) return fn(s, args);
    return previous ? previous(args) : undefined;
  });
}

{
  const adapters = mockHandler("dap_adapters");
  registerMock("dap_adapters", async (args) => [...(((await adapters?.(args)) as AdapterInfo[] | undefined) ?? []), ...ADAPTERS]);
  const dapStart = mockHandler("dap_start");
  registerMock("dap_start", (args) => {
    const request = args.request as LaunchRequest;
    return request.adapter.startsWith("pwa-") ? start(request) : dapStart?.(args);
  });
  const restart = mockHandler("dap_restart");
  registerMock("dap_restart", (args) => {
    const s = own(args);
    if (!s) return restart?.(args);
    s.pc = 0;
    resume(s, () => (breakpointAt(s) ? stopped(s, "breakpoint") : run(s, "continue")));
    return s.info;
  });
  for (const cmd of ["debug_status", "debug_sessions"]) {
    const base = mockHandler(cmd)!;
    registerMock(cmd, async (args) => {
      const list = (await base(args)) as SessionInfo[] | { sessions: SessionInfo[] };
      const mine = [...sessions.values()].map((s) => s.info);
      return Array.isArray(list) ? [...list, ...mine] : { ...list, sessions: [...list.sessions, ...mine] };
    });
  }
}

route("dap_threads", (s) => (s.info.parentId ? [{ id: 1, name: "Main Thread" }] : []));
route("dap_loaded_sources", () => [
  { name: "pricing.ts", path: `${ROOT}/${PRICING}`, file: PRICING },
  { name: "server.ts", path: `${ROOT}/${SERVER}`, file: SERVER },
  { name: "store.ts", path: `${ROOT}/src/orders/store.ts`, file: "src/orders/store.ts" },
]);
route("debug_step", (s, args) => {
  if (s.info.status !== "paused") throw new Error("the program is not suspended");
  const kind = String(args.kind);
  const mode = kind === "run" ? "continue" : kind === "stepOver" ? "next" : kind === "stepOut" ? "stepOut" : "stepIn";
  resume(s, () => run(s, mode));
});
route("debug_pause", (s) => (s.info.status === "running" ? stopped(s, "pause") : undefined));
route("debug_stop", (s) => end(s.info.parentId ?? s.info.id));
route("debug_detach", (s) => end(s.info.parentId ?? s.info.id));
route("debug_run_to_cursor", (s, args) => {
  const line = Number(args.line);
  const file = String(args.file ?? "");
  resume(s, () => {
    const target = TRACE.findIndex((x, n) => n > s.pc && x.frames[0][2] === line && (!file || x.frames[0][1] === file));
    if (target < 0) return run(s, "continue");
    s.pc = target;
    stopped(s, "breakpoint");
  });
});
route("debug_stack", (s): Frame[] =>
  s.info.status !== "paused"
    ? []
    : TRACE[s.pc].frames.map(([where, file, line], level) => ({
        level,
        file,
        fileUri: file ? `${ROOT}/${file}` : where === "emit" ? "node:events" : "node:_http_server",
        line,
        where,
        id: level + 1,
      })),
);
route("debug_contexts", () => [
  { id: 0, name: "Local" },
  { id: 1, name: "Closure" },
  { id: 2, name: "Global" },
]);
route("debug_variables", (s, args) => {
  const level = Number(args.depth ?? 0);
  const context = Number(args.context ?? 0);
  const frame = TRACE[s.pc].frames[level];
  if (context === 1) return frame?.[1] === PRICING ? [prop("TAX_RATE", "0.19")] : [prop("PORT", "3000"), prop("orderTotal", "ƒ orderTotal(order)"), prop("findOrder", "ƒ findOrder(id)")];
  if (context === 2) return [prop("process", "process {version: 'v22.11.0', …}"), prop("globalThis", "global {…}")];
  if (level > 0 && frame?.[1] === SERVER)
    return [prop("req", "IncomingMessage"), prop("res", "ServerResponse"), prop("url", "URL"), prop("match", "(2) ['/orders/1042/total', '1042']"), prop("order", ORDER, ORDER_REF)];
  if (!frame?.[1]) return [];
  return Object.entries(TRACE[s.pc].locals).map(([k, v]) => prop(k, v === ORDER ? ORDER : v, v === ORDER ? ORDER_REF : undefined));
});
route("debug_property", (_s, args) => ({
  name: String(args.fullname),
  fullname: String(args.fullname),
  type: "dap",
  hasChildren: true,
  numChildren: ORDER_CHILDREN.length,
  page: 0,
  pageSize: 0,
  children: ORDER_CHILDREN.map(([k, v]) => ({ ...prop(k, v), fullname: `${String(args.fullname)}.${k}` })),
  reference: Number(args.reference),
}));
route("debug_eval", (s, args) => {
  const e = String(args.expression).trim();
  const locals = TRACE[s.pc].locals;
  if (e in locals) return { ...prop(e, locals[e]), reference: locals[e] === ORDER ? ORDER_REF : undefined, hasChildren: locals[e] === ORDER };
  if (e === "subtotal - discount" && locals.discount !== "undefined") return prop(e, "53.973");
  if (e === "TAX_RATE") return prop(e, "0.19");
  if (/^order\.(id|voucher|items)$/.test(e)) return prop(e, ORDER_CHILDREN.find(([k]) => `order.${k}` === e)![1]);
  if (/^-?\d+(\.\d+)?$/.test(e)) return prop(e, e);
  throw new Error(`ReferenceError: ${e} is not defined`);
});
route("debug_set_value", () => undefined);
route("debug_complete", (s, args) => {
  const prefix = /[A-Za-z_$][\w$]*$/.exec(String(args.text))?.[0] ?? "";
  return Object.keys(TRACE[s.pc].locals)
    .filter((n) => n.startsWith(prefix))
    .map((n) => ({ label: n, insert: n, detail: "local" }));
});

// Screenshot scenarios can start sessions directly.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeJsDebugMock?: unknown }).__forgeJsDebugMock = { start };
}
