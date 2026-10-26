/**
 * Browser-mode Java debugging: java-debug sessions (`dap_start` with adapter `java`)
 * attached to the demo's `App.main` (`?project=java`), which calls `Inventory.remove`.
 * Threads, frames, variables (objects expand), stepping, breakpoints with conditions and
 * Evaluate Expression are simulated in memory; other sessions go to the C program of
 * dapMock.ts and Xdebug's mock.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { Breakpoint, BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { AdapterInfo, LaunchRequest } from "@/features/debug/dapApi";
import { JAVA_PKG } from "./demoJavaProject";

const APP = `${JAVA_PKG}/App.java`;
const INVENTORY = `${JAVA_PKG}/Inventory.java`;
const ROOT = "/home/anna/Projects/inventory-service";

const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const call = <T>(cmd: string, args: Record<string, unknown> = {}) => mockHandler(cmd)!(args) as T;

const ADAPTER: AdapterInfo = { type: "java", label: "Java (java-debug)", languages: ["java"], launch: false, attachProcess: false, coreDump: false };

interface Item {
  sku: string;
  name: string;
  quantity: number;
}
interface JVars {
  inventory?: Item[];
  sku?: string;
  quantity?: number;
  current?: Item;
  left?: number;
  low?: Item[];
}
/** [method, file, 1-based line] top frame first, and the visible locals. */
interface JStep {
  frames: [string, string, number][];
  vars: JVars;
}

function trace(): JStep[] {
  const out: JStep[] = [];
  const anvil = { sku: "SKU-1001", name: "Anvil", quantity: 4 };
  let hammer = { sku: "SKU-2002", name: "Hammer", quantity: 12 };
  const main = (line: number, vars: JVars) => out.push({ frames: [["App.main(String[])", APP, line]], vars });
  const remove = (line: number, vars: JVars) =>
    out.push({ frames: [["Inventory.remove(String, int)", INVENTORY, line], ["App.main(String[])", APP, 12]], vars });
  main(9, {});
  main(10, { inventory: [] });
  main(11, { inventory: [anvil] });
  main(12, { inventory: [anvil, hammer] });
  remove(18, { inventory: [anvil, hammer], sku: "SKU-2002", quantity: 5 });
  remove(19, { inventory: [anvil, hammer], sku: "SKU-2002", quantity: 5, current: hammer });
  remove(22, { inventory: [anvil, hammer], sku: "SKU-2002", quantity: 5, current: hammer });
  remove(23, { inventory: [anvil, hammer], sku: "SKU-2002", quantity: 5, current: hammer, left: 7 });
  remove(26, { inventory: [anvil, hammer], sku: "SKU-2002", quantity: 5, current: hammer, left: 7 });
  hammer = { ...hammer, quantity: 7 };
  main(14, { inventory: [anvil, hammer] });
  main(15, { inventory: [anvil, hammer], low: [anvil] });
  main(16, { inventory: [anvil, hammer], low: [anvil] });
  main(17, { inventory: [anvil, hammer], low: [anvil] });
  main(19, { inventory: [anvil, hammer], low: [anvil] });
  return out;
}

interface JavaSession {
  info: SessionInfo;
  steps: JStep[];
  pc: number;
}

const sessions = new Map<string, JavaSession>();
let seq = 0;
const isJava = (args: Record<string, unknown>) => sessions.has(String(args.sessionId ?? ""));
const step = (s: JavaSession) => s.steps[s.pc];
const top = (s: JavaSession) => step(s).frames[0];

// Variable references: 100 + level*20 + slot.
const OBJECT_BASE = 5000;
const objects = new Map<number, Property[]>();
let nextRef = OBJECT_BASE;

function leaf(name: string, value: string, classname: string, container?: number): Property {
  return { name, fullname: name, type: "dap", classname, value, hasChildren: false, numChildren: 0, page: 0, pageSize: 0, children: [], container };
}

function object(name: string, value: string, classname: string, children: Property[]): Property {
  const reference = nextRef++;
  objects.set(reference, children);
  return { name, fullname: name, type: "dap", classname, value, hasChildren: children.length > 0, numChildren: children.length, page: 0, pageSize: 0, children: [], reference };
}

const itemProp = (name: string, item: Item, id: number) =>
  object(name, `StockItem@${id} "StockItem[sku=${item.sku}, name=${item.name}, quantity=${item.quantity}]"`, "com.acme.inventory.StockItem", [
    leaf("sku", `"${item.sku}"`, "java.lang.String"),
    leaf("name", `"${item.name}"`, "java.lang.String"),
    leaf("quantity", String(item.quantity), "int"),
  ]);

const listProp = (name: string, list: Item[], kind: string, id: number) =>
  object(name, `${kind}@${id}  size = ${list.length}`, `java.util.${kind}`, list.map((item, i) => itemProp(kind === "LinkedHashMap" ? `"${item.sku}"` : `[${i}]`, item, 1210 + i)));

function locals(s: JavaSession, level: number): Property[] {
  const frame = step(s).frames[level];
  const v = step(s).vars;
  if (!frame) return [];
  if (frame[0].startsWith("Inventory.remove")) {
    const out = [
      object("this", "Inventory@1203", "com.acme.inventory.Inventory", [listProp("items", v.inventory ?? [], "LinkedHashMap", 1204)]),
      leaf("sku", `"${v.sku}"`, "java.lang.String"),
      leaf("quantity", String(v.quantity), "int"),
    ];
    if (v.current) out.push(itemProp("current", v.current, 1211));
    if (v.left !== undefined) out.push(leaf("left", String(v.left), "int"));
    return out;
  }
  const out: Property[] = [object("args", "String[0]@1199", "java.lang.String[]", [])];
  if (v.inventory) out.push(object("inventory", "Inventory@1203", "com.acme.inventory.Inventory", [listProp("items", v.inventory, "LinkedHashMap", 1204)]));
  if (v.low) out.push(listProp("low", v.low, "ArrayList", 1230));
  return out;
}

function evaluate(s: JavaSession, expression: string, level: number): Property {
  const e = expression.trim();
  const all = locals(s, level);
  const direct = all.find((p) => p.name === e);
  if (direct) return direct;
  const call = /^(\w+)\.(\w+)\(\)$/.exec(e);
  if (call) {
    const owner = all.find((p) => p.name === call[1]);
    const field = owner?.reference ? objects.get(owner.reference)?.find((c) => c.name === call[2]) : undefined;
    if (field) return { ...field, name: e, fullname: e };
  }
  const v = step(s).vars;
  if (/^current\.quantity\(\)\s*-\s*quantity$/.test(e) && v.current && v.quantity !== undefined) return leaf(e, String(v.current.quantity - v.quantity), "int");
  if (/^-?\d+$/.test(e)) return leaf(e, e, "int");
  throw new Error(`${e} cannot be resolved to a variable`);
}

function features(): SessionInfo["features"] {
  return {
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
    loadedSources: false,
    attached: true,
    exceptionFilters: [
      { filter: "uncaught", label: "Uncaught Exceptions", default: true, supportsCondition: true },
      { filter: "caught", label: "Caught Exceptions", default: false, supportsCondition: true },
    ],
  };
}

function output(s: JavaSession, stream: string, text: string) {
  emit({ type: "output", sessionId: s.info.id, stream, text });
}

function stopped(s: JavaSession, reason: SessionInfo["reason"]) {
  const [, file, line] = top(s);
  s.info = { ...s.info, status: "paused", reason, exception: null, threadId: 1, location: { file, fileUri: `${ROOT}/${file}`, line } };
  emit({ type: "session", session: s.info });
}

function end(s: JavaSession, code: number) {
  output(s, "system", `\nDisconnected from the target VM, address: '127.0.0.1:47011', transport: 'socket'\n\nProcess finished with exit code ${code}\n`);
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

function hitAt(s: JavaSession): Breakpoint | null {
  const snap = call<BreakpointsSnapshot>("debug_breakpoints");
  if (snap.muted) return null;
  const [, file, line] = top(s);
  for (const bp of snap.breakpoints.filter((b) => b.enabled && b.kind === "line" && b.file === file && b.line === line)) {
    if (bp.condition?.trim()) {
      try {
        if (evaluate(s, bp.condition, 0).value !== "true") continue;
      } catch {
        continue;
      }
    }
    return bp;
  }
  return null;
}

type Mode = "continue" | "next" | "stepIn" | "stepOut";

function run(s: JavaSession, mode: Mode) {
  const depth = step(s).frames.length;
  for (;;) {
    const [, , line] = top(s);
    if (top(s)[1] === APP && line === 15) output(s, "stdout", "2 items, 1 low on stock\n");
    if (top(s)[1] === APP && line === 17) output(s, "stdout", "Reorder Anvil (4 left)\n");
    if (s.pc + 1 >= s.steps.length) return end(s, 0);
    s.pc += 1;
    if (hitAt(s)) return stopped(s, "breakpoint");
    const d = step(s).frames.length;
    if (mode === "stepIn" || (mode === "next" && d <= depth) || (mode === "stepOut" && d < depth)) return stopped(s, "step");
  }
}

function resume(s: JavaSession, fn: () => void) {
  s.info = { ...s.info, status: "running", location: null, reason: null, exception: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => sessions.has(s.info.id) && fn(), 120);
}

function start(request: LaunchRequest): SessionInfo {
  const id = `dap-java-${++seq}`;
  const configuration = (request.configuration ?? {}) as { hostName?: string; port?: number };
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "java",
    engine: ADAPTER.label,
    fileUri: `${ROOT}/${APP}`,
    file: APP,
    peer: `${configuration.hostName ?? "127.0.0.1"}:${configuration.port ?? 47011}`,
    serverName: request.name ?? "App",
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: "java",
    features: features(),
  };
  const s: JavaSession = { info, steps: trace(), pc: 0 };
  sessions.set(id, s);
  emit({ type: "session", session: info });
  output(s, "system", `Connected to the target VM, address: '${info.peer}', transport: 'socket'\n`);
  setTimeout(() => {
    if (!sessions.has(id)) return;
    if (hitAt(s)) return stopped(s, "breakpoint");
    run(s, "continue");
  }, 300);
  return info;
}

// ---- routing --------------------------------------------------------------------------------

const prevAdapters = mockHandler("dap_adapters")!;
registerMock("dap_adapters", async (args) => [...((await prevAdapters(args)) as AdapterInfo[]), ADAPTER]);

const prevStart = mockHandler("dap_start")!;
registerMock("dap_start", (args) => {
  const request = args.request as LaunchRequest;
  return request.adapter === "java" ? start(request) : prevStart(args);
});

function route(cmd: string, fn: (s: JavaSession, args: Record<string, unknown>) => unknown) {
  const prev = mockHandler(cmd);
  registerMock(cmd, (args) => (isJava(args) ? fn(sessions.get(String(args.sessionId))!, args) : prev?.(args)));
}

const withJava = (cmd: string) => {
  const prev = mockHandler(cmd)!;
  registerMock(cmd, async (args) => {
    const base = (await prev(args)) as SessionInfo[] | { sessions: SessionInfo[] };
    const mine = [...sessions.values()].map((s) => s.info);
    return Array.isArray(base) ? [...base, ...mine] : { ...base, sessions: [...base.sessions, ...mine] };
  });
};
withJava("debug_status");
withJava("debug_sessions");

route("dap_threads", () => [
  { id: 1, name: "main" },
  { id: 2, name: "Reference Handler" },
  { id: 3, name: "Finalizer" },
  { id: 4, name: "Signal Dispatcher" },
]);
route("dap_restart", (s) => {
  s.pc = 0;
  resume(s, () => (hitAt(s) ? stopped(s, "breakpoint") : run(s, "continue")));
  return s.info;
});
route("debug_step", (s, args) => {
  if (s.info.status !== "paused") throw new Error("the VM is not suspended");
  const kind = String(args.kind);
  const mode: Mode = kind === "run" ? "continue" : kind === "stepOver" ? "next" : kind === "stepOut" ? "stepOut" : "stepIn";
  resume(s, () => run(s, mode));
});
route("debug_pause", () => undefined);
route("debug_stop", (s) => end(s, 130));
route("debug_detach", (s) => end(s, 0));
route("debug_run_to_cursor", (s, args) => {
  const line = Number(args.line);
  resume(s, () => {
    const target = s.steps.findIndex((x, n) => n > s.pc && x.frames[0][2] === line);
    if (target < 0) return run(s, "continue");
    s.pc = target;
    stopped(s, "breakpoint");
  });
});
route("debug_stack", (s, args): Frame[] => {
  if (Number(args.threadId ?? 1) !== 1) {
    return [{ level: 0, file: null, fileUri: "", line: 0, where: "Object.wait(long)", id: 61 }];
  }
  return step(s).frames.map(([where, file, line], level) => ({ level, file, fileUri: `${ROOT}/${file}`, line, where, id: level + 1 }));
});
route("debug_contexts", () => [{ id: 0, name: "Local" }]);
route("debug_variables", (s, args) => {
  objects.clear();
  nextRef = OBJECT_BASE;
  return locals(s, Number(args.depth ?? 0));
});
route("debug_property", (_s, args) => {
  const reference = Number(args.reference);
  const children = objects.get(reference) ?? [];
  return { name: String(args.fullname), fullname: String(args.fullname), type: "dap", hasChildren: children.length > 0, numChildren: children.length, page: 0, pageSize: 0, children, reference };
});
route("debug_eval", (s, args) => evaluate(s, String(args.expression), Number(args.depth ?? 0)));
route("debug_set_value", (s, args) => {
  const name = String(args.name ?? args.fullname);
  const value = Number(String(args.value).trim());
  if (!Number.isInteger(value)) throw new Error("Type mismatch: cannot convert to int");
  const v = step(s).vars;
  if (name === "quantity" && v.quantity !== undefined) step(s).vars = { ...v, quantity: value };
  else if (name === "left" && v.left !== undefined) step(s).vars = { ...v, left: value };
  else throw new Error(`cannot set '${name}'`);
});
route("debug_complete", (s, args) => {
  const prefix = /[A-Za-z_]\w*$/.exec(String(args.text))?.[0] ?? "";
  return locals(s, Number(args.depth ?? 0))
    .filter((p) => p.name.startsWith(prefix))
    .map((p) => ({ label: p.name, insert: p.name, detail: p.classname ?? "" }));
});

// Screenshot scenarios can start sessions directly.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeJavaDebugMock?: unknown }).__forgeJavaDebugMock = { start };
}
