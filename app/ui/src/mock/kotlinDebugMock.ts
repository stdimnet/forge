/**
 * Browser-mode Kotlin debugging: kotlin-lsp debug sessions (`dap_start` with adapter
 * `kotlin`) attached to the demo's `main` (`?project=kotlin`), suspended in the consumer
 * coroutine at `service.checkout(order)` (Main.kt:20) when a breakpoint is there. Threads,
 * frames, variables and Evaluate Expression are simulated; evaluating the coroutine dump
 * expression returns a `DebugProbes` dump that the Coroutines tab parses in Rust.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { AdapterInfo, LaunchRequest } from "@/features/debug/dapApi";
import { KT_PKG } from "./demoKotlinProject";

const MAIN = `${KT_PKG}/Main.kt`;
const SERVICE = `${KT_PKG}/OrderService.kt`;
const ROOT = "/home/anna/Projects/orders-service";
const STOP_LINE = 20;

const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const call = <T>(cmd: string, args: Record<string, unknown> = {}) => mockHandler(cmd)!(args) as T;

const ADAPTER: AdapterInfo = { type: "kotlin", label: "Kotlin (kotlin-lsp)", languages: ["kt", "kts"], launch: false, attachProcess: false, coreDump: false };

/** What `DebugProbes.dumpCoroutines` prints at the stop. */
const DUMP = `Coroutines dump 2026/10/26 10:15:02

Coroutine "consumer#3":StandaloneCoroutine{Active}@1b6d3586, state: RUNNING
\tat com.acme.orders.MainKt$main$1$consumer$1.invokeSuspend(Main.kt:20)
\t(Coroutine creation stacktrace)
\tat kotlinx.coroutines.BuildersKt__Builders_commonKt.launch(Builders.common.kt:47)
\tat com.acme.orders.MainKt$main$1.invokeSuspend(Main.kt:18)

Coroutine "producer#2":StandaloneCoroutine{Active}@5e9f23b4, state: SUSPENDED
\tat kotlinx.coroutines.DelayKt.delay(Delay.kt:121)
\tat com.acme.orders.MainKt$main$1$producer$1.invokeSuspend(Main.kt:13)
\t(Coroutine creation stacktrace)
\tat com.acme.orders.MainKt$main$1.invokeSuspend(Main.kt:10)

Coroutine "coroutine#1":BlockingCoroutine{Active}@4554617c, state: SUSPENDED
\tat kotlinx.coroutines.JobSupport.join(JobSupport.kt:1564)
\tat kotlinx.coroutines.AwaitKt.joinAll(Await.kt:72)
\tat com.acme.orders.MainKt$main$1.invokeSuspend(Main.kt:25)
`;

const quote = (s: string) => `"${s.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, "\\n").replace(/\t/g, "\\t")}"`;

interface KSession {
  info: SessionInfo;
  paused: boolean;
}

const sessions = new Map<string, KSession>();
let seq = 0;
const isKotlin = (args: Record<string, unknown>) => sessions.has(String(args.sessionId ?? ""));

const leaf = (name: string, value: string, classname: string): Property => ({ name, fullname: name, type: "dap", classname, value, hasChildren: false, numChildren: 0, page: 0, pageSize: 0, children: [] });
const objects = new Map<number, Property[]>();
let nextRef = 7000;
function object(name: string, value: string, classname: string, children: Property[]): Property {
  const reference = nextRef++;
  objects.set(reference, children);
  return { name, fullname: name, type: "dap", classname, value, hasChildren: children.length > 0, numChildren: children.length, page: 0, pageSize: 0, children: [], reference };
}

function locals(level: number): Property[] {
  const lineItem = object("[0]", 'LineItem@1290 "LineItem(sku=anvil, quantity=2, priceCents=4990)"', "com.acme.orders.LineItem", [
    leaf("sku", '"anvil"', "java.lang.String"),
    leaf("quantity", "2", "int"),
    leaf("priceCents", "4990", "long"),
  ]);
  const order = object("order", 'Order@1288 "Order(id=3, items=[LineItem(sku=anvil, quantity=2, priceCents=4990)])"', "com.acme.orders.Order", [
    leaf("id", "3", "int"),
    object("items", "SingletonList@1289  size = 1", "java.util.Collections$SingletonList", [lineItem]),
  ]);
  if (level === 1) return [object("this", "OrderService@1301", "com.acme.orders.OrderService", []), order];
  return [
    object("$this$launch", 'StandaloneCoroutine@1b6d3586 "\\"consumer#3\\":StandaloneCoroutine{Active}@1b6d3586"', "kotlinx.coroutines.StandaloneCoroutine", []),
    order,
    object("service", "OrderService@1301", "com.acme.orders.OrderService", []),
  ];
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
    exceptionFilters: [{ filter: "uncaught", label: "Uncaught Exceptions", default: true, supportsCondition: true }],
  };
}

function output(s: KSession, stream: string, text: string) {
  emit({ type: "output", sessionId: s.info.id, stream, text });
}

function end(s: KSession, code: number) {
  output(s, "system", `\nDisconnected from the target VM, address: '127.0.0.1:47120', transport: 'socket'\n\nProcess finished with exit code ${code}\n`);
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

function breakpointAtStop(): boolean {
  const snap = call<BreakpointsSnapshot>("debug_breakpoints");
  return !snap.muted && snap.breakpoints.some((b) => b.enabled && b.kind === "line" && b.file === MAIN && b.line === STOP_LINE);
}

function pause(s: KSession) {
  s.paused = true;
  s.info = { ...s.info, status: "paused", reason: "breakpoint", exception: null, threadId: 21, location: { file: MAIN, fileUri: `${ROOT}/${MAIN}`, line: STOP_LINE } };
  emit({ type: "session", session: s.info });
}

function start(request: LaunchRequest): SessionInfo {
  const id = `dap-kotlin-${++seq}`;
  const configuration = (request.configuration ?? {}) as { hostName?: string; port?: number };
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "kotlin",
    engine: ADAPTER.label,
    fileUri: `${ROOT}/${MAIN}`,
    file: MAIN,
    peer: `${configuration.hostName ?? "127.0.0.1"}:${configuration.port ?? 47120}`,
    serverName: request.name ?? "MainKt",
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: "kotlin",
    features: features(),
  };
  const s: KSession = { info, paused: false };
  sessions.set(id, s);
  emit({ type: "session", session: info });
  output(s, "system", `Connected to the target VM, address: '${info.peer}', transport: 'socket'\n`);
  setTimeout(() => {
    if (!sessions.has(id)) return;
    output(s, "stdout", "Order #1: 99.80 EUR\nOrder #2: 99.80 EUR\n");
    if (breakpointAtStop()) return pause(s);
    output(s, "stdout", "Order #3: 99.80 EUR\nOrder #4: 99.80 EUR\nOrder #5: 99.80 EUR\n");
    end(s, 0);
  }, 400);
  return info;
}

const prevAdapters = mockHandler("dap_adapters")!;
registerMock("dap_adapters", async (args) => [...((await prevAdapters(args)) as AdapterInfo[]), ADAPTER]);

const prevStart = mockHandler("dap_start")!;
registerMock("dap_start", (args) => ((args.request as LaunchRequest).adapter === "kotlin" ? start(args.request as LaunchRequest) : prevStart(args)));

function route(cmd: string, fn: (s: KSession, args: Record<string, unknown>) => unknown) {
  const prev = mockHandler(cmd);
  registerMock(cmd, (args) => (isKotlin(args) ? fn(sessions.get(String(args.sessionId))!, args) : prev?.(args)));
}

const withKotlin = (cmd: string) => {
  const prev = mockHandler(cmd)!;
  registerMock(cmd, async (args) => {
    const base = (await prev(args)) as SessionInfo[] | { sessions: SessionInfo[] };
    const mine = [...sessions.values()].map((s) => s.info);
    return Array.isArray(base) ? [...base, ...mine] : { ...base, sessions: [...base.sessions, ...mine] };
  });
};
withKotlin("debug_status");
withKotlin("debug_sessions");

route("dap_threads", () => [
  { id: 1, name: "main @coroutine#1" },
  { id: 21, name: "DefaultDispatcher-worker-1 @consumer#3" },
  { id: 22, name: "DefaultDispatcher-worker-2" },
  { id: 2, name: "Reference Handler" },
  { id: 3, name: "Finalizer" },
]);
route("dap_restart", (s) => s.info);
route("debug_step", (s) => {
  if (!s.paused) throw new Error("the VM is not suspended");
  s.paused = false;
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => {
    output(s, "stdout", "Order #3: 99.80 EUR\nOrder #4: 99.80 EUR\nOrder #5: 99.80 EUR\n");
    end(s, 0);
  }, 300);
});
route("debug_pause", () => undefined);
route("debug_stop", (s) => end(s, 130));
route("debug_detach", (s) => end(s, 0));
route("debug_stack", (_s, args): Frame[] => {
  if (Number(args.threadId ?? 21) !== 21) return [{ level: 0, file: null, fileUri: "", line: 0, where: "Unsafe.park(boolean, long)", id: 91 }];
  return [
    { level: 0, file: MAIN, fileUri: `${ROOT}/${MAIN}`, line: STOP_LINE, where: "invokeSuspend:20, MainKt$main$1$consumer$1", id: 1 },
    { level: 1, file: null, fileUri: "", line: 0, where: "resumeWith:33, BaseContinuationImpl (kotlin.coroutines.jvm.internal)", id: 2 },
    { level: 2, file: null, fileUri: "", line: 0, where: "run:100, DispatchedTask (kotlinx.coroutines)", id: 3 },
    { level: 3, file: null, fileUri: "", line: 0, where: "runWorker:586, CoroutineScheduler$Worker (kotlinx.coroutines.scheduling)", id: 4 },
  ];
});
route("debug_contexts", () => [{ id: 0, name: "Local" }]);
route("debug_variables", (_s, args) => {
  objects.clear();
  nextRef = 7000;
  return locals(Number(args.depth ?? 0));
});
route("debug_property", (_s, args) => {
  const children = objects.get(Number(args.reference)) ?? [];
  return { name: String(args.fullname), fullname: String(args.fullname), type: "dap", hasChildren: children.length > 0, numChildren: children.length, page: 0, pageSize: 0, children, reference: Number(args.reference) };
});
route("debug_eval", (_s, args) => {
  const e = String(args.expression).trim();
  if (e.includes("DebugProbes.dumpCoroutines")) return leaf(e, quote(DUMP), "java.lang.String");
  if (e.includes("DebugProbesImpl")) {
    return leaf(e, quote('[CoroutineInfo(state=RUNNING,context=[CoroutineName(consumer), CoroutineId(3), "consumer#3":StandaloneCoroutine{Active}@1b6d3586, Dispatchers.Default])]'), "java.lang.String");
  }
  const found = locals(Number(args.depth ?? 0)).find((p) => p.name === e);
  if (found) return found;
  if (e === "order.id") return leaf(e, "3", "int");
  throw new Error(`Unresolved reference: ${e}`);
});
route("debug_complete", (_s, args) => {
  const prefix = /[A-Za-z_$]\w*$/.exec(String(args.text))?.[0] ?? "";
  return locals(0)
    .filter((p) => p.name.startsWith(prefix))
    .map((p) => ({ label: p.name, insert: p.name, detail: p.classname ?? "" }));
});

/** The source of the service, for the frames of the Coroutines view. */
export const KOTLIN_SERVICE_FILE = SERVICE;

// Screenshot scenarios can start sessions directly.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeKotlinDebugMock?: unknown }).__forgeKotlinDebugMock = { start };
}
