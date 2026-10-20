/**
 * Browser-mode PHP debugger. "Start Listening" is followed by a Laravel request that
 * suspends at the breakpoint in OrderController::show; stepping walks through the method
 * (and into InvoiceService::settle). Everything stays in memory.
 */
import { emitMock, registerMock } from "@/ipc";
import type {
  Breakpoint,
  BreakpointSpec,
  BreakpointsSnapshot,
  DebugEvent,
  Frame,
  Property,
  ServerConfig,
  SessionInfo,
  StepKind,
} from "@/features/debug/api";

const CONTROLLER = "app/Http/Controllers/OrderController.php";
const INVOICES = "app/Services/InvoiceService.php";
const REMOTE_ROOT = "/var/www/html";

const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const uri = (file: string) => `file://${REMOTE_ROOT}/${file}`;

// ---- values -----------------------------------------------------------------------------

type Child = [name: string, value: Node, facet?: string];
interface Node {
  type: string;
  classname?: string;
  value?: string;
  children?: Child[];
}

const s = (value: string): Node => ({ type: "string", value });
const i = (n: number): Node => ({ type: "int", value: String(n) });
const f = (n: number): Node => ({ type: "float", value: String(n) });
const b = (v: boolean): Node => ({ type: "bool", value: v ? "1" : "0" });
const nul: Node = { type: "null" };
const arr = (children: Child[]): Node => ({ type: "array", children });
const obj = (classname: string, children: Child[]): Node => ({ type: "object", classname, children });
const assoc = (o: Record<string, Node>): Node => arr(Object.entries(o).map(([k, v]) => [k, v]));

function model(classname: string, table: string, attributes: Record<string, Node>, relations: Child[] = []): Node {
  const attrs = assoc(attributes);
  return obj(classname, [
    ["connection", s("mysql"), "protected"],
    ["table", s(table), "protected"],
    ["primaryKey", s("id"), "protected"],
    ["keyType", s("int"), "protected"],
    ["incrementing", b(true), "public"],
    ["with", arr([]), "protected"],
    ["perPage", i(15), "protected"],
    ["exists", b(true), "public"],
    ["wasRecentlyCreated", b(false), "public"],
    ["attributes", attrs, "protected"],
    ["original", attrs, "protected"],
    ["changes", arr([]), "protected"],
    ["casts", assoc({ total: s("decimal:2"), status: s("App\\Models\\OrderStatus") }), "protected"],
    ["relations", arr(relations), "protected"],
    ["timestamps", b(true), "public"],
    ["hidden", arr([]), "protected"],
    ["fillable", arr([]), "protected"],
    ["guarded", arr([["0", s("*")]]), "protected"],
  ]);
}

const line = (id: number, product: string, price: number, quantity: number) =>
  model("App\\Models\\OrderLine", "order_lines", { id: i(id), order_id: i(1042), product: s(product), price: f(price), quantity: i(quantity) });

const orderLines = [line(1, "Forge Hammer", 49.9, 1), line(2, "Anvil Stand", 35, 2), line(3, "Ember Gloves", 15, 2)];
const lines = obj("Illuminate\\Database\\Eloquent\\Collection", [["items", arr(orderLines.map((l, n) => [String(n), l])), "protected"], ["escapeWhenCastingToString", b(false), "protected"]]);
const user = model("App\\Models\\User", "users", {
  id: i(7),
  name: s("Anna Weber"),
  email: s("anna@acme.dev"),
  email_verified_at: s("2026-03-02 09:14:11"),
  is_vip: b(true),
  created_at: s("2026-01-12 10:00:00"),
});
const order = model(
  "App\\Models\\Order",
  "orders",
  {
    id: i(1042),
    user_id: i(7),
    status: s("pending"),
    total: s("149.90"),
    currency: s("EUR"),
    created_at: s("2026-10-18 08:12:44"),
    updated_at: s("2026-10-18 08:12:44"),
  },
  [["lines", lines]],
);
const request = obj("Illuminate\\Http\\Request", [
  ["json", nul, "protected"],
  ["convertedFiles", nul, "protected"],
  ["userResolver", obj("Closure", []), "protected"],
  ["routeResolver", obj("Closure", []), "protected"],
  ["attributes", obj("Symfony\\Component\\HttpFoundation\\ParameterBag", [["parameters", assoc({ _route: s("orders.show") }), "protected"]]), "public"],
  ["request", obj("Symfony\\Component\\HttpFoundation\\InputBag", [["parameters", arr([]), "protected"]]), "public"],
  ["query", obj("Symfony\\Component\\HttpFoundation\\InputBag", [["parameters", assoc({ include: s("lines") }), "protected"]]), "public"],
  ["server", obj("Symfony\\Component\\HttpFoundation\\ServerBag", [["parameters", assoc({ SERVER_NAME: s("acme-shop.test"), REQUEST_METHOD: s("GET"), REQUEST_URI: s("/orders/1042?include=lines") }), "protected"]]), "public"],
  ["headers", obj("Symfony\\Component\\HttpFoundation\\HeaderBag", [["headers", assoc({ host: arr([["0", s("acme-shop.test")]]), accept: arr([["0", s("application/json")]]) }), "protected"]]), "public"],
  ["content", nul, "protected"],
  ["method", s("GET"), "protected"],
  ["requestUri", s("/orders/1042?include=lines"), "protected"],
  ["pathInfo", s("/orders/1042"), "protected"],
  ["format", nul, "protected"],
]);
const invoiceService = obj("App\\Services\\InvoiceService", [["checkout", obj("App\\Services\\Checkout", [["currency", s("EUR"), "private"], ["taxRate", f(0.19), "private"]]), "private readonly"]]);
const controller = obj("App\\Http\\Controllers\\OrderController", [["invoices", invoiceService, "private readonly"]]);

function childName(parent: Node, name: string, parentFull: string): string {
  if (parent.type === "array") return /^\d+$/.test(name) ? `${parentFull}[${name}]` : `${parentFull}['${name}']`;
  return `${parentFull}->${name}`;
}

/** DBGp-like property: children one level deep (grandchildren are fetched on expand). */
function toProperty(name: string, fullname: string, node: Node, facet?: string, withChildren = true): Property {
  const kids = node.children ?? [];
  return {
    name,
    fullname,
    type: node.type,
    classname: node.classname,
    facet,
    value: node.value,
    size: node.type === "string" ? node.value?.length : undefined,
    hasChildren: kids.length > 0,
    numChildren: kids.length,
    page: 0,
    pageSize: 100,
    children: withChildren ? kids.map(([n, v, fc]) => toProperty(n, childName(node, n, fullname), v, fc, false)) : [],
  };
}

// ---- the scripted request -----------------------------------------------------------------

interface Step {
  file: string;
  line: number;
  vars: [string, Node][];
  /** Where Step Into goes from here (index into STEPS). */
  into?: number;
  /** Where Step Out goes from here. */
  out?: number;
  /** Where Step Over goes (default: next). */
  over?: number;
}

const base: [string, Node][] = [
  ["$this", controller],
  ["$request", request],
  ["$order", order],
];
const STEPS: Step[] = [
  { file: CONTROLLER, line: 20, vars: base },
  { file: CONTROLLER, line: 22, vars: base },
  { file: CONTROLLER, line: 23, vars: [...base, ["$user", user]] },
  { file: CONTROLLER, line: 24, vars: [...base, ["$user", user], ["$lines", lines]] },
  { file: CONTROLLER, line: 25, vars: [...base, ["$user", user], ["$lines", lines], ["$total", f(149.9)]] },
  { file: CONTROLLER, line: 27, vars: [...base, ["$user", user], ["$lines", lines], ["$total", f(149.9)], ["$discount", f(0.1)]] },
  { file: CONTROLLER, line: 28, vars: [...base, ["$user", user], ["$lines", lines], ["$total", f(149.9)], ["$discount", f(0.1)]], into: 8, over: 7 },
  { file: CONTROLLER, line: 31, vars: [...base, ["$user", user], ["$lines", lines], ["$total", f(149.9)], ["$discount", f(0.1)]] },
  // InvoiceService::settle
  { file: INVOICES, line: 20, vars: [["$this", invoiceService], ["$order", order]], out: 7 },
  { file: INVOICES, line: 24, vars: [["$this", invoiceService], ["$order", order]], out: 7 },
  { file: INVOICES, line: 25, vars: [["$this", invoiceService], ["$order", order]], out: 7 },
  { file: INVOICES, line: 27, vars: [["$this", invoiceService], ["$order", order], ["$lines", arr([["0", f(49.9)], ["1", i(70)], ["2", i(30)]])]], out: 7, over: 7 },
];
const START = 3; // breakpoint on `$total = …`

const vendor = (path: string, line: number, where: string): Frame => ({
  level: 0,
  file: `vendor/laravel/framework/src/Illuminate/${path}`,
  fileUri: uri(`vendor/laravel/framework/src/Illuminate/${path}`),
  line,
  where,
});
const LARAVEL_FRAMES: Frame[] = [
  vendor("Routing/ControllerDispatcher.php", 46, "Illuminate\\Routing\\ControllerDispatcher->dispatch"),
  vendor("Routing/Route.php", 265, "Illuminate\\Routing\\Route->runController"),
  vendor("Routing/Route.php", 211, "Illuminate\\Routing\\Route->run"),
  vendor("Routing/Router.php", 808, "Illuminate\\Routing\\Router->{closure:Illuminate\\Routing\\Router::runRouteWithinStack():807}"),
  vendor("Pipeline/Pipeline.php", 170, "Illuminate\\Pipeline\\Pipeline->{closure:Illuminate\\Pipeline\\Pipeline::prepareDestination():168}"),
  vendor("Routing/Middleware/SubstituteBindings.php", 51, "Illuminate\\Routing\\Middleware\\SubstituteBindings->handle"),
  vendor("Pipeline/Pipeline.php", 209, "Illuminate\\Pipeline\\Pipeline->{closure:Illuminate\\Pipeline\\Pipeline::carry():184}"),
  vendor("Routing/Router.php", 807, "Illuminate\\Routing\\Router->runRouteWithinStack"),
  vendor("Routing/Router.php", 786, "Illuminate\\Routing\\Router->runRoute"),
  vendor("Routing/Router.php", 750, "Illuminate\\Routing\\Router->dispatchToRoute"),
  vendor("Routing/Router.php", 739, "Illuminate\\Routing\\Router->dispatch"),
  vendor("Foundation/Http/Kernel.php", 201, "Illuminate\\Foundation\\Http\\Kernel->{closure:Illuminate\\Foundation\\Http\\Kernel::dispatchToRouter():198}"),
  vendor("Foundation/Http/Kernel.php", 175, "Illuminate\\Foundation\\Http\\Kernel->sendRequestThroughRouter"),
  vendor("Foundation/Http/Kernel.php", 144, "Illuminate\\Foundation\\Http\\Kernel->handle"),
  vendor("Foundation/Application.php", 1219, "Illuminate\\Foundation\\Application->handleRequest"),
  { level: 0, file: "public/index.php", fileUri: uri("public/index.php"), line: 20, where: "{main}" },
];

function framesFor(stepIdx: number): Frame[] {
  const st = STEPS[stepIdx];
  const top: Frame[] = [];
  if (st.file === INVOICES) {
    top.push({ level: 0, file: INVOICES, fileUri: uri(INVOICES), line: st.line, where: "App\\Services\\InvoiceService->settle" });
    top.push({ level: 0, file: CONTROLLER, fileUri: uri(CONTROLLER), line: 28, where: "App\\Http\\Controllers\\OrderController->show" });
  } else {
    top.push({ level: 0, file: CONTROLLER, fileUri: uri(CONTROLLER), line: st.line, where: "App\\Http\\Controllers\\OrderController->show" });
  }
  return [...top, ...LARAVEL_FRAMES].map((fr, level) => ({ ...fr, level }));
}

// ---- state --------------------------------------------------------------------------------

let listening = false;
let port = 9003;
let seq = 0;
let nextBp = 1;
let watches: string[] = ["$order->status", "count($lines)"];
let servers: ServerConfig[] = [
  {
    id: "acme-shop-test",
    name: "acme-shop.test",
    host: "acme-shop.test",
    port: 80,
    debugger: "Xdebug",
    usePathMappings: true,
    mappings: [
      { local: "", remote: REMOTE_ROOT },
      { local: "vendor", remote: "/var/www/vendor" },
    ],
  },
  { id: "docker-cli", name: "docker-cli", host: "localhost", port: 80, debugger: "Xdebug", usePathMappings: true, mappings: [{ local: "", remote: "/app" }] },
];
const bp = (spec: BreakpointSpec): Breakpoint => ({ id: nextBp++, kind: spec.kind ?? "line", file: spec.file, line: spec.line, exception: spec.exception, condition: spec.condition, enabled: spec.enabled ?? true });
const state: BreakpointsSnapshot = {
  muted: false,
  breakpoints: [
    bp({ file: CONTROLLER, line: 24 }),
    bp({ file: CONTROLLER, line: 28, condition: "$order->total > 100" }),
    bp({ file: "app/Services/Checkout.php", line: 13, enabled: false }),
    bp({ kind: "exception", exception: "\\Illuminate\\Database\\QueryException" }),
  ],
};

interface MockSession {
  info: SessionInfo;
  step: number;
}
const sessions = new Map<string, MockSession>();

const snapshot = (): BreakpointsSnapshot => JSON.parse(JSON.stringify(state));
const emitBreakpoints = () => emit({ type: "breakpoints", snapshot: snapshot() });

function sessionInfo(id: string, overrides: Partial<SessionInfo> = {}): SessionInfo {
  return {
    id,
    ideKey: "PHPSTORM",
    language: "PHP",
    engine: "Xdebug 3.4.1",
    fileUri: uri("public/index.php"),
    file: "public/index.php",
    peer: "172.18.0.3:53814",
    serverName: "acme-shop.test",
    host: "acme-shop.test",
    port: 80,
    serverId: "acme-shop-test",
    status: "running",
    location: null,
    reason: null,
    exception: null,
    ...overrides,
  };
}

function suspend(s: MockSession, stepIdx: number, reason: SessionInfo["reason"]) {
  s.step = stepIdx;
  const st = STEPS[stepIdx];
  s.info = { ...s.info, status: "paused", reason, location: { file: st.file, fileUri: uri(st.file), line: st.line } };
  emit({ type: "session", session: s.info });
}

function finish(s: MockSession) {
  emit({ type: "output", sessionId: s.info.id, stream: "stdout", text: '{"order":{"id":1042,"status":"pending","user":{"id":7,"name":"Anna Weber"}},"total":134.91}\n' });
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

function connect(): void {
  const id = `xdebug-${++seq}`;
  const s: MockSession = { info: sessionInfo(id), step: START };
  sessions.set(id, s);
  emit({ type: "session", session: s.info });
  setTimeout(() => {
    if (sessions.has(id)) suspend(s, START, state.muted ? "pause" : "breakpoint");
  }, 250);
}

/** An unknown server, for the "Incoming Connection From Xdebug" dialog. */
function connectUnknown(): void {
  const id = `xdebug-${++seq}`;
  const info = sessionInfo(id, {
    status: "awaitingAcceptance",
    serverName: "staging.acme-shop.dev",
    host: "staging.acme-shop.dev",
    port: 8080,
    serverId: null,
    fileUri: "file:///srv/acme-shop/releases/42/public/index.php",
    file: null,
    peer: "10.0.4.17:41022",
    ideKey: "FORGE",
  });
  sessions.set(id, { info, step: START });
  emit({ type: "incoming", session: info });
}

function session(args: Record<string, unknown>): MockSession {
  const s = sessions.get(String(args.sessionId));
  if (!s) throw new Error(`no debug session ${String(args.sessionId)}`);
  return s;
}

function isBreakpointHit(stepIdx: number): boolean {
  if (state.muted) return false;
  const st = STEPS[stepIdx];
  return state.breakpoints.some((b) => b.enabled && b.kind === "line" && b.file === st.file && b.line === st.line);
}

function advance(s: MockSession, kind: StepKind): void {
  const cur = STEPS[s.step];
  let next: number | null;
  switch (kind) {
    case "stepInto":
    case "forceStepInto":
      next = cur.into ?? (s.step + 1 < STEPS.length && STEPS[s.step + 1].file === cur.file ? s.step + 1 : null);
      break;
    case "stepOut":
      next = cur.out ?? null;
      break;
    case "stepOver":
      next = cur.over ?? (s.step + 1 < STEPS.length && STEPS[s.step + 1].file === cur.file ? s.step + 1 : null);
      break;
    default: {
      next = null;
      for (let n = s.step + 1; n < STEPS.length && STEPS[n].file === CONTROLLER; n++) {
        if (isBreakpointHit(n)) {
          next = n;
          break;
        }
      }
    }
  }
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => {
    if (!sessions.has(s.info.id)) return;
    if (next == null) finish(s);
    else suspend(s, next, kind === "run" ? "breakpoint" : "step");
  }, 120);
}

function varsOf(s: MockSession): [string, Node][] {
  return STEPS[s.step].vars;
}

/** Find a node by DBGp fullname (`$order->attributes['id']`). */
function lookup(s: MockSession, fullname: string): Node | null {
  const m = /^(\$\w+)(.*)$/.exec(fullname.trim());
  if (!m) return null;
  let node = varsOf(s).find(([n]) => n === m[1])?.[1] ?? null;
  const re = /->(\w+)|\[(?:'([^']*)'|"([^"]*)"|(\d+))\]/g;
  let rest = m[2];
  let match: RegExpExecArray | null;
  let consumed = 0;
  while (node && (match = re.exec(rest))) {
    if (match.index !== consumed) return null;
    consumed = match.index + match[0].length;
    const key = match[1] ?? match[2] ?? match[3] ?? match[4];
    let child = node.children?.find(([n]) => n === key)?.[1];
    // Eloquent magic: $order->status reads attributes['status'].
    if (!child && match[1]) child = node.children?.find(([n]) => n === "attributes")?.[1].children?.find(([n]) => n === key)?.[1];
    node = child ?? null;
  }
  rest = rest.slice(consumed);
  return rest.trim() ? null : node;
}

function evaluate(s: MockSession, expression: string): Property {
  const e = expression.trim().replace(/;$/, "");
  const direct = lookup(s, e);
  if (direct) return toProperty(e, e, direct);
  const count = /^count\((.+)\)$/.exec(e);
  if (count) {
    const n = lookup(s, count[1]);
    if (n?.type === "array" || n?.classname?.endsWith("Collection")) {
      const items = n.type === "array" ? n.children ?? [] : n.children?.find(([k]) => k === "items")?.[1].children ?? [];
      return toProperty(e, e, i(items.length));
    }
  }
  const upper = /^strtoupper\((.+)\)$/.exec(e);
  if (upper) {
    const n = lookup(s, upper[1]);
    if (n?.type === "string") return toProperty(e, e, s_(n.value!.toUpperCase()));
  }
  // Arithmetic over numeric variables, e.g. `$total * (1 - $discount)`.
  let undefinedVar: string | null = null;
  const replaced = e.replace(/\$\w+(?:->\w+|\[[^\]]+\])*/g, (v) => {
    const n = lookup(s, v);
    if (!n || !["int", "float", "string"].includes(n.type) || Number.isNaN(Number(n.value))) {
      undefinedVar = v;
      return "0";
    }
    return `(${Number(n.value)})`;
  });
  if (undefinedVar) throw new Error(`Undefined variable ${undefinedVar}`);
  if (/^[\d\s+\-*/().]+$/.test(replaced)) {
    const value = Function(`"use strict"; return (${replaced});`)() as number;
    return toProperty(e, e, Number.isInteger(value) ? i(value) : f(Math.round(value * 1e6) / 1e6));
  }
  throw new Error(`Error evaluating code: syntax error, unexpected end of file`);
}
const s_ = s;

// ---- handlers -----------------------------------------------------------------------------

registerMock("debug_status", () => ({ listening, port, sessions: [...sessions.values()].map((x) => x.info) }));
registerMock("debug_listen_start", () => {
  listening = true;
  emit({ type: "listening", listening: true, port });
  if (![...sessions.values()].length) setTimeout(connect, 350);
  return port;
});
registerMock("debug_listen_stop", () => {
  listening = false;
  emit({ type: "listening", listening: false, port });
});
registerMock("debug_sessions", () => [...sessions.values()].map((x) => x.info));
registerMock("debug_accept_connection", (args) => {
  const s = session(args);
  const server = args.serverConfig as ServerConfig;
  servers = [...servers.filter((x) => x.id !== server.id), server];
  s.info = { ...s.info, serverId: server.id, status: "running" };
  emit({ type: "session", session: s.info });
  setTimeout(() => suspend(s, START, "breakpoint"), 200);
});
registerMock("debug_ignore_connection", (args) => {
  const s = session(args);
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
});
registerMock("debug_step", (args) => {
  const s = session(args);
  if (s.info.status !== "paused") throw new Error("the script is not suspended");
  advance(s, args.kind as StepKind);
});
registerMock("debug_pause", (args) => {
  const s = session(args);
  if (s.info.status === "running") suspend(s, s.step, "pause");
});
registerMock("debug_stop", (args) => {
  const s = session(args);
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
});
registerMock("debug_detach", (args) => {
  const s = session(args);
  finish(s);
});
registerMock("debug_run_to_cursor", (args) => {
  const s = session(args);
  const target = STEPS.findIndex((st, n) => n > s.step && st.file === args.file && st.line >= Number(args.line));
  s.info = { ...s.info, status: "running", location: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => (target < 0 ? finish(s) : suspend(s, target, "breakpoint")), 120);
});
registerMock("debug_stack", (args) => framesFor(session(args).step));
registerMock("debug_contexts", () => [
  { id: 0, name: "Locals" },
  { id: 1, name: "Superglobals" },
  { id: 2, name: "User defined constants" },
]);
registerMock("debug_variables", (args) => {
  const s = session(args);
  const depth = Number(args.depth ?? 0);
  if (depth > 0) {
    // Caller frames: the controller for InvoiceService, framework internals otherwise.
    if (STEPS[s.step].file === INVOICES && depth === 1) return STEPS[6].vars.map(([n, v]) => toProperty(n, n, v, undefined, false));
    return [toProperty("$request", "$request", request, undefined, false)];
  }
  return varsOf(s).map(([n, v]) => toProperty(n, n, v, undefined, false));
});
registerMock("debug_property", (args) => {
  const s = session(args);
  const fullname = String(args.fullname);
  const node = lookup(s, fullname);
  if (!node) throw new Error(`can not get property ${fullname}`);
  return toProperty(fullname.split(/->|\[/).pop()!.replace(/['"\]]/g, ""), fullname, node);
});
registerMock("debug_eval", (args) => evaluate(session(args), String(args.expression)));
registerMock("debug_set_value", () => undefined);
registerMock("debug_breakpoints", () => snapshot());
registerMock("debug_breakpoint_add", (args) => {
  const spec = args.spec as BreakpointSpec;
  const existing = state.breakpoints.find((x) => x.kind === "line" && spec.kind !== "exception" && x.file === spec.file && x.line === spec.line);
  if (existing) return existing;
  const created = bp(spec);
  state.breakpoints.push(created);
  emitBreakpoints();
  return created;
});
registerMock("debug_breakpoint_update", (args) => {
  const updated = args.breakpoint as Breakpoint;
  state.breakpoints = state.breakpoints.map((x) => (x.id === updated.id ? { ...updated } : x));
  emitBreakpoints();
  return updated;
});
registerMock("debug_breakpoint_remove", (args) => {
  state.breakpoints = state.breakpoints.filter((x) => x.id !== Number(args.id));
  emitBreakpoints();
});
registerMock("debug_breakpoint_toggle", (args) => {
  const file = String(args.file);
  const ln = Number(args.line);
  const existing = state.breakpoints.find((x) => x.kind === "line" && x.file === file && x.line === ln);
  if (existing) {
    state.breakpoints = state.breakpoints.filter((x) => x !== existing);
    emitBreakpoints();
    return null;
  }
  const created = bp({ file, line: ln });
  state.breakpoints.push(created);
  emitBreakpoints();
  return created;
});
registerMock("debug_breakpoints_mute", (args) => {
  state.muted = !!args.muted;
  emitBreakpoints();
});
registerMock("debug_watches", () => [...watches]);
registerMock("debug_watches_set", (args) => {
  watches = [...(args.watches as string[])];
});
registerMock("debug_servers", () => JSON.parse(JSON.stringify(servers)));
registerMock("debug_servers_save", (args) => {
  servers = JSON.parse(JSON.stringify(args.servers));
});
registerMock("debug_launch_env", (args) => ({
  XDEBUG_MODE: "debug",
  XDEBUG_SESSION: "FORGE",
  XDEBUG_CONFIG: `client_host=127.0.0.1 client_port=${port} idekey=FORGE`,
  ...(args.serverName ? { PHP_IDE_CONFIG: `serverName=${String(args.serverName)}` } : {}),
}));

// Screenshot scenarios can simulate connections.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeDebugMock?: unknown }).__forgeDebugMock = { connect, connectUnknown };
}
