/**
 * Browser-mode debug sessions of the C++ demo (`?project=cpp`): debugging a CMake target of
 * `shapes` under GDB or LLDB stops at the first enabled line breakpoint in `src/main.cpp`
 * (or runs to the end), with the frames of `main` and its locals rendered like the STL
 * pretty printers do: `std::vector<shapes::Polygon>` with its elements, the corners of each
 * polygon and the `std::vector<double>` of areas. Wraps the `debug_*` mocks of `dapMock.ts`
 * for its own sessions only.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { LaunchRequest } from "@/features/debug/dapApi";
import { cppDemoName } from "./demoCppProject";

const ROOT = `/home/anna/Projects/${cppDemoName}`;
const FILE = "src/main.cpp";
const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const call = <T>(cmd: string, args: Record<string, unknown> = {}) => mockHandler(cmd)!(args) as T;

/** Lines of `main` the session steps through (1-based lines of main.cpp). */
const LINES = [15, 16, 17, 18, 19];

interface CppSession {
  info: SessionInfo;
  at: number;
  /** Loop iterations done (areas pushed). */
  iteration: number;
}

const sessions = new Map<string, CppSession>();
let seq = 0;
const isCpp = (args: Record<string, unknown>) => sessions.has(String(args.sessionId ?? ""));

const prop = (name: string, value: string, classname: string, reference?: number, numChildren = 0, memoryReference?: string): Property => ({
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
  memoryReference,
});

const POLYGONS = [
  { corners: [[0, 0], [2, 0], [2, 2], [0, 2]], area: 4 },
  { corners: [[0, 0], [4, 0], [0, 3]], area: 6 },
  { corners: [[0, 0], [3, 0], [3, 3], [0, 3]], area: 9 },
];

const point = (p: number[]) => `{x = ${p[0]}, y = ${p[1]}}`;

function locals(s: CppSession): Property[] {
  const areas = POLYGONS.slice(0, s.iteration).map((p) => p.area);
  const out = [
    prop("argc", "1", "int"),
    prop("argv", "0x7fffffffd9a8", "char **", 3001, 1, "0x7fffffffd9a8"),
    prop("polygons", "std::vector of length 3, capacity 3", "std::vector<shapes::Polygon>", 3100, 3, "0x7fffffffd840"),
    prop("areas", `std::vector of length ${areas.length}, capacity ${areas.length ? 4 : 0}${areas.length ? ` = {${areas.join(", ")}}` : ""}`, "std::vector<double>", areas.length ? 3200 : undefined, areas.length, "0x7fffffffd860"),
  ];
  if (LINES[s.at] >= 16 && s.iteration < 3) out.push(prop("p", `@0x55555556f2c0: {corners_ = std::vector of length ${POLYGONS[s.iteration].corners.length}}`, "const shapes::Polygon &", 3110 + s.iteration, 1));
  return out;
}

function children(reference: number, s: CppSession): Property[] {
  if (reference === 3001) return [prop("*argv", `0x7fffffffde12 "${ROOT}/cmake-build-debug/shapes_app"`, "char *")];
  if (reference === 3100) return POLYGONS.map((p, i) => prop(`[${i}]`, `{corners_ = std::vector of length ${p.corners.length}, capacity ${p.corners.length}}`, "shapes::Polygon", 3110 + i, 1));
  if (reference >= 3110 && reference < 3120) {
    const p = POLYGONS[reference - 3110];
    return [prop("corners_", `std::vector of length ${p.corners.length}, capacity ${p.corners.length} = {${p.corners.map(point).join(", ")}}`, "std::vector<shapes::Point>", 3120 + (reference - 3110) * 10, p.corners.length)];
  }
  if (reference >= 3120 && reference < 3160) {
    const p = POLYGONS[Math.floor((reference - 3120) / 10)];
    return p.corners.map((c, i) => prop(`[${i}]`, point(c), "shapes::Point", 3300 + (reference - 3120) + i, 2));
  }
  if (reference >= 3300 && reference < 3400) {
    const base = reference - 3300;
    const p = POLYGONS[Math.floor(base / 10)];
    const c = p?.corners[base % 10] ?? [0, 0];
    return [prop("x", String(c[0]), "double"), prop("y", String(c[1]), "double")];
  }
  if (reference === 3200) return POLYGONS.slice(0, s.iteration).map((p, i) => prop(`[${i}]`, String(p.area), "double"));
  return [];
}

function frames(s: CppSession): Frame[] {
  return [
    { level: 0, file: FILE, fileUri: `${ROOT}/${FILE}`, line: LINES[s.at], where: "main(int, char**)", id: 1, address: "0x0000555555556f3a" },
    { level: 1, file: null, fileUri: "/lib/x86_64-linux-gnu/libc.so.6", line: 0, where: "__libc_start_call_main(main=…, argc=1, argv=…)", id: 2, address: "0x00007ffff7829d90" },
    { level: 2, file: null, fileUri: "/lib/x86_64-linux-gnu/libc.so.6", line: 0, where: "__libc_start_main_impl", id: 3, address: "0x00007ffff7829e40" },
    { level: 3, file: null, fileUri: `${ROOT}/cmake-build-debug/shapes_app`, line: 0, where: "_start", id: 4, address: "0x0000555555556b25" },
  ];
}

function stopped(s: CppSession, reason: SessionInfo["reason"]): void {
  s.info = { ...s.info, status: "paused", reason, threadId: 1, location: { file: FILE, fileUri: `${ROOT}/${FILE}`, line: LINES[s.at] } };
  emit({ type: "session", session: s.info });
}

function end(s: CppSession, code: number): void {
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
  const id = `dap-cpp-${++seq}`;
  const gdb = request.adapter === "gdb";
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "cpp",
    engine: gdb ? "GDB" : "LLDB",
    fileUri: `${ROOT}/${FILE}`,
    file: FILE,
    peer: gdb ? "gdb -i dap" : "lldb-dap",
    serverName: request.name ?? "shapes_app",
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: request.adapter,
    threadId: 1,
    features: {
      threads: true,
      readMemory: true,
      writeMemory: true,
      disassemble: true,
      registers: true,
      dataBreakpoints: true,
      functionBreakpoints: true,
      conditionalBreakpoints: true,
      hitConditionalBreakpoints: true,
      logPoints: true,
      instructionStepping: true,
      goto: false,
      restart: false,
      completions: true,
      setVariable: true,
      modules: true,
      loadedSources: false,
      attached: false,
      exceptionFilters: [{ filter: "cpp_throw", label: "C++: on throw", default: false, supportsCondition: false }],
    },
  };
  const s: CppSession = { info, at: 0, iteration: 0 };
  sessions.set(id, s);
  emit({ type: "session", session: info });
  emit({ type: "output", sessionId: id, stream: "system", text: `${request.program ?? ""}\n` });
  setTimeout(() => {
    if (!sessions.has(id)) return;
    const lines = breakpointLines();
    const hit = LINES.findIndex((l) => lines.includes(l));
    if (hit < 0) {
      emit({ type: "output", sessionId: id, stream: "stdout", text: "4 corners, area 4.00, perimeter 8.00\n3 corners, area 6.00, perimeter 12.00\n4 corners, area 9.00, perimeter 12.00\nmean area: 6.33333\nmedian area: 6\n" });
      return end(s, 0);
    }
    s.at = hit;
    // The loop body breakpoint stops in the second round, after the first area was added.
    s.iteration = LINES[hit] >= 16 ? 1 : 0;
    stopped(s, "breakpoint");
  }, 350);
  return info;
}

// ---- handlers ------------------------------------------------------------------------------

const isShapes = (request: LaunchRequest) => (request.program ?? "").includes(`/Projects/${cppDemoName}/`);

const previousStart = mockHandler("dap_start")!;
registerMock("dap_start", (args) => {
  const request = args.request as LaunchRequest;
  return isShapes(request) ? start(request) : previousStart(args);
});

function route(cmd: string, fn: (s: CppSession, args: Record<string, unknown>) => unknown) {
  const previous = mockHandler(cmd)!;
  registerMock(cmd, (args) => (isCpp(args) ? fn(sessions.get(String(args.sessionId))!, args) : previous(args)));
}

const withCpp = (cmd: string) => {
  const previous = mockHandler(cmd)!;
  registerMock(cmd, async (args) => {
    const base = (await previous(args)) as SessionInfo[] | { sessions: SessionInfo[] };
    const cpp = [...sessions.values()].map((s) => s.info);
    return Array.isArray(base) ? [...base, ...cpp] : { ...base, sessions: [...base.sessions, ...cpp] };
  });
};
withCpp("debug_status");
withCpp("debug_sessions");

route("debug_step", (s, args) => {
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => {
    if (!sessions.has(s.info.id)) return;
    if (args.kind === "run" || s.at + 1 >= LINES.length) return end(s, 0);
    s.at += 1;
    if (LINES[s.at] === 18) s.iteration = Math.min(3, s.iteration + 1);
    stopped(s, "step");
  }, 120);
});
route("dap_threads", (s) => [{ id: 1, name: s.info.serverName ?? "shapes_app" }]);
route("debug_pause", () => undefined);
route("debug_stop", (s) => end(s, 9));
route("debug_detach", (s) => end(s, 0));
route("debug_stack", (s) => frames(s));
route("debug_contexts", () => [
  { id: 0, name: "Locals" },
  { id: 1, name: "Registers" },
]);
route("debug_variables", (s, args) =>
  Number(args.context ?? 0) === 1
    ? [prop("rip", "0x0000555555556f3a", "uint64_t"), prop("rsp", "0x00007fffffffd820", "uint64_t"), prop("rbp", "0x00007fffffffd990", "uint64_t")]
    : Number(args.depth ?? 0) === 0
      ? locals(s)
      : [],
);
route("debug_property", (s, args) => {
  const reference = Number(args.reference);
  const list = children(reference, s);
  return { name: String(args.fullname), fullname: String(args.fullname), type: "dap", hasChildren: true, numChildren: list.length, page: 0, pageSize: 0, children: list, reference };
});
route("debug_eval", (s, args) => {
  const e = String(args.expression).trim();
  const known: Record<string, [string, string]> = {
    "polygons.size()": ["3", "std::size_t"],
    "polygons[0].area()": ["4", "double"],
    "areas.size()": [String(s.iteration), "std::size_t"],
  };
  if (known[e]) return { ...prop(e, known[e][0], known[e][1]), container: undefined };
  const local = locals(s).find((p) => p.name === e);
  if (local) return { ...local, container: undefined };
  throw new Error(`No symbol "${e.split(/\W/)[0] || e}" in current context.`);
});
route("debug_complete", (s, args) => {
  const prefix = /[A-Za-z_]\w*$/.exec(String(args.text))?.[0] ?? "";
  return locals(s)
    .filter((l) => l.name.startsWith(prefix))
    .map((l) => ({ label: l.name, insert: l.name, detail: l.classname }));
});
