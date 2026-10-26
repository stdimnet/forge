/**
 * Browser-mode debugpy sessions for the FastAPI demo: attaching to a debug run (adapter
 * `python`) stops at the first enabled line breakpoint in `src/shop_api/pricing.py` (or
 * runs to the end), with frames, locals of `Cart.total` and the debug console evaluating
 * in the frame. Wraps the `debug_*` mocks of `dapMock.ts` for its own sessions only.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { AdapterInfo, LaunchRequest } from "@/features/debug/dapApi";
import { pythonDemoName } from "./demoPythonProject";

const ROOT = `/home/anna/Projects/${pythonDemoName}`;
const FILE = "src/shop_api/pricing.py";
const TEST = "tests/test_pricing.py";
const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const call = <T>(cmd: string, args: Record<string, unknown> = {}) => mockHandler(cmd)!(args) as T;

const ADAPTER: AdapterInfo = {
  type: "python",
  label: "Python (debugpy)",
  languages: ["py", "pyw"],
  launch: true,
  attachProcess: true,
  coreDump: false,
  command: `${ROOT}/.venv/bin/python`,
};

/** `Cart.total`, line by line (1-based lines of pricing.py). */
const LINES = [26, 27];

interface PySession {
  info: SessionInfo;
  at: number;
}

const sessions = new Map<string, PySession>();
let seq = 0;
const isPython = (args: Record<string, unknown>) => sessions.has(String(args.sessionId ?? ""));

const prop = (name: string, value: string, classname: string, reference?: number, numChildren = 0): Property => ({
  name,
  fullname: name,
  type: "dap",
  classname,
  value,
  hasChildren: !!reference,
  numChildren,
  page: 0,
  pageSize: 0,
  children: [],
  reference,
  container: 1000,
});

function locals(s: PySession): Property[] {
  const line = LINES[s.at];
  const out = [
    prop("self", "Cart(items=[('mug', Decimal('12.50'), 2)])", "Cart", 2001, 1),
    prop("tax_rate", "Decimal('0.19')", "Decimal"),
  ];
  if (line >= 27) out.push(prop("net", "Decimal('25.00')", "Decimal"));
  return out;
}

const CHILDREN: Record<number, Property[]> = {
  2001: [prop("items", "[('mug', Decimal('12.50'), 2)]", "list", 2002, 1)],
  2002: [prop("0", "('mug', Decimal('12.50'), 2)", "tuple", 2003, 3)],
  2003: [prop("0", "'mug'", "str"), prop("1", "Decimal('12.50')", "Decimal"), prop("2", "2", "int")],
};

function frames(s: PySession): Frame[] {
  return [
    { level: 0, file: FILE, fileUri: `${ROOT}/${FILE}`, line: LINES[s.at], where: "total", id: 1 },
    { level: 1, file: TEST, fileUri: `${ROOT}/${TEST}`, line: 31, where: "test_total_includes_tax", id: 2 },
    { level: 2, file: null, fileUri: `${ROOT}/.venv/lib/python3.12/site-packages/_pytest/python.py`, line: 159, where: "pytest_pyfunc_call", id: 3 },
    { level: 3, file: null, fileUri: `${ROOT}/.venv/lib/python3.12/site-packages/pluggy/_callers.py`, line: 103, where: "_multicall", id: 4 },
  ];
}

function evaluate(s: PySession, expression: string): Property {
  const e = expression.trim();
  const known: Record<string, [string, string]> = {
    "net * (1 + tax_rate)": ["Decimal('29.7500')", "Decimal"],
    "len(self.items)": ["1", "int"],
    "self.items[0]": ["('mug', Decimal('12.50'), 2)", "tuple"],
    "self.items": ["[('mug', Decimal('12.50'), 2)]", "list"],
  };
  if (known[e]) return { ...prop(e, known[e][0], known[e][1]), container: undefined };
  const local = locals(s).find((p) => p.name === e);
  if (local) return { ...local, container: undefined };
  if (/^-?\d+(\.\d+)?$/.test(e)) return { ...prop(e, e, e.includes(".") ? "float" : "int"), container: undefined };
  throw new Error(`NameError: name '${e.split(/\W/)[0] || e}' is not defined`);
}

function stopped(s: PySession, reason: SessionInfo["reason"]): void {
  s.info = { ...s.info, status: "paused", reason, threadId: 1, location: { file: FILE, fileUri: `${ROOT}/${FILE}`, line: LINES[s.at] } };
  emit({ type: "session", session: s.info });
}

function end(s: PySession, code: number): void {
  emit({ type: "output", sessionId: s.info.id, stream: "system", text: `\nProcess finished with exit code ${code}\n` });
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

function breakpointLines(): number[] {
  const snap = call<BreakpointsSnapshot>("debug_breakpoints");
  if (snap.muted) return [];
  return snap.breakpoints.filter((b) => b.enabled && b.kind === "line" && b.file === FILE).map((b) => b.line ?? 0);
}

function start(request: LaunchRequest): SessionInfo {
  const id = `dap-py-${++seq}`;
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "python",
    engine: ADAPTER.label,
    fileUri: `${ROOT}/${TEST}`,
    file: TEST,
    peer: request.mode === "attachServer" ? `${request.host}:${request.port}` : "debugpy",
    serverName: request.name ?? "pytest",
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: "python",
    features: {
      threads: true,
      readMemory: false,
      writeMemory: false,
      disassemble: false,
      registers: false,
      dataBreakpoints: false,
      functionBreakpoints: true,
      conditionalBreakpoints: true,
      hitConditionalBreakpoints: true,
      logPoints: true,
      instructionStepping: false,
      goto: true,
      restart: false,
      completions: true,
      setVariable: true,
      modules: true,
      loadedSources: true,
      attached: request.mode !== "launch",
      exceptionFilters: [
        { filter: "raised", label: "Raised Exceptions", default: false, supportsCondition: true },
        { filter: "uncaught", label: "Uncaught Exceptions", default: true, supportsCondition: true },
        { filter: "userUnhandled", label: "User Uncaught Exceptions", default: false, supportsCondition: true },
      ],
    },
  };
  const s: PySession = { info, at: 0 };
  sessions.set(id, s);
  emit({ type: "session", session: info });
  emit({ type: "output", sessionId: id, stream: "system", text: `Connected to debugpy at ${info.peer}\n` });
  setTimeout(() => {
    if (!sessions.has(id)) return;
    const lines = breakpointLines();
    const hit = LINES.findIndex((l) => lines.includes(l));
    if (hit < 0) return end(s, 0);
    s.at = hit;
    stopped(s, "breakpoint");
  }, 350);
  return info;
}

// ---- handlers ------------------------------------------------------------------------------

const previousAdapters = mockHandler("dap_adapters")!;
registerMock("dap_adapters", async (args) => [...((await previousAdapters(args)) as AdapterInfo[]).filter((a) => a.type !== "python"), ADAPTER]);

const previousStart = mockHandler("dap_start")!;
registerMock("dap_start", (args) => {
  const request = args.request as LaunchRequest;
  return request.adapter === "python" ? start(request) : previousStart(args);
});

/** Route a `debug_*` command to Python sessions, everything else to the earlier mocks. */
function route(cmd: string, fn: (s: PySession, args: Record<string, unknown>) => unknown) {
  const previous = mockHandler(cmd)!;
  registerMock(cmd, (args) => (isPython(args) ? fn(sessions.get(String(args.sessionId))!, args) : previous(args)));
}

const withPython = (cmd: string) => {
  const previous = mockHandler(cmd)!;
  registerMock(cmd, async (args) => {
    const base = (await previous(args)) as SessionInfo[] | { sessions: SessionInfo[] };
    const py = [...sessions.values()].map((s) => s.info);
    return Array.isArray(base) ? [...base, ...py] : { ...base, sessions: [...base.sessions, ...py] };
  });
};
withPython("debug_status");
withPython("debug_sessions");

route("debug_step", (s, args) => {
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => {
    if (!sessions.has(s.info.id)) return;
    if (args.kind === "run" || s.at + 1 >= LINES.length) {
      emit({ type: "output", sessionId: s.info.id, stream: "stdout", text: "PASSED\n" });
      return end(s, 0);
    }
    s.at += 1;
    stopped(s, "step");
  }, 120);
});
route("debug_pause", () => undefined);
route("debug_stop", (s) => end(s, 143));
route("debug_detach", (s) => end(s, 0));
route("debug_stack", (s) => frames(s));
route("debug_contexts", () => [
  { id: 0, name: "Locals" },
  { id: 1, name: "Globals" },
]);
route("debug_variables", (s, args) =>
  Number(args.context ?? 0) === 1
    ? [prop("ROUND_HALF_UP", "'ROUND_HALF_UP'", "str"), prop("Decimal", "<class 'decimal.Decimal'>", "type")]
    : Number(args.depth ?? 0) === 0
      ? locals(s)
      : [prop("cart", "Cart(items=[('mug', Decimal('12.50'), 2)])", "Cart", 2001, 1), prop("self", "<tests.test_pricing.TestCart object at 0x7f3a2b9c1d10>", "TestCart")],
);
route("debug_property", (_s, args) => {
  const reference = Number(args.reference);
  const children = CHILDREN[reference] ?? [];
  return { name: String(args.fullname), fullname: String(args.fullname), type: "dap", hasChildren: true, numChildren: children.length, page: 0, pageSize: 0, children, reference };
});
route("debug_eval", (s, args) => evaluate(s, String(args.expression)));
route("debug_complete", (s, args) => {
  const prefix = /[A-Za-z_]\w*$/.exec(String(args.text))?.[0] ?? "";
  return locals(s)
    .filter((l) => l.name.startsWith(prefix))
    .map((l) => ({ label: l.name, insert: l.name, detail: l.classname }));
});
