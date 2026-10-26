/**
 * Browser-mode Debug Adapter Protocol sessions: a simulated C program
 * (`scripts/native/demo.c`, the same program as `forge_dap::fake`) under "LLDB", with
 * threads, variables, watchpoints, logpoints, memory, disassembly and registers. Attach to
 * Process lists a few fake processes; attaching to "demo" (or any start) runs the program
 * to the first breakpoint. Wraps the `debug_*` mocks for `dap-<n>` session ids; Xdebug
 * sessions go to `debugMock.ts`. Everything stays in memory.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { Breakpoint, BreakpointSpec, BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo, StepKind } from "@/features/debug/api";
import type { AdapterInfo, Instruction, LaunchRequest, MemoryRow, ProcessInfo } from "@/features/debug/dapApi";

const FILE = "scripts/native/demo.c";
const ABS = `/home/anna/Projects/acme-shop/${FILE}`;
const PROGRAM = "/home/anna/Projects/acme-shop/scripts/native/demo";

const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const call = <T>(cmd: string, args: Record<string, unknown> = {}) => mockHandler(cmd)!(args) as T;

const ADAPTERS: AdapterInfo[] = [
  {
    type: "lldb",
    label: "LLDB",
    languages: ["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "m", "mm", "rs", "swift", "zig"],
    launch: true,
    attachProcess: true,
    coreDump: true,
    command: "lldb-dap",
  },
  {
    type: "gdb",
    label: "GDB",
    languages: ["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "rs", "go", "f", "f90", "adb", "ads", "d"],
    launch: true,
    attachProcess: true,
    coreDump: false,
    command: "gdb",
  },
];

const PROCESSES: ProcessInfo[] = [
  { pid: 1, name: "systemd", commandLine: "/sbin/init splash", user: "root" },
  { pid: 812, name: "nginx", commandLine: "nginx: master process /usr/sbin/nginx -g daemon on;", user: "root" },
  { pid: 1204, name: "php-fpm8.3", commandLine: "php-fpm: master process (/etc/php/8.3/fpm/php-fpm.conf)", user: "root" },
  { pid: 1230, name: "php-fpm8.3", commandLine: "php-fpm: pool www", user: "www-data" },
  { pid: 1231, name: "php-fpm8.3", commandLine: "php-fpm: pool www", user: "www-data" },
  { pid: 2051, name: "postgres", commandLine: "/usr/lib/postgresql/16/bin/postgres -D /var/lib/postgresql/16/main", user: "postgres" },
  { pid: 3377, name: "redis-server", commandLine: "/usr/bin/redis-server 127.0.0.1:6379", user: "redis" },
  { pid: 4242, name: "demo", commandLine: `${PROGRAM} --watch`, user: "anna" },
  { pid: 5110, name: "node", commandLine: "node node_modules/.bin/vite --port 5173", user: "anna" },
  { pid: 6021, name: "queue-worker", commandLine: "php artisan queue:work --tries=3", user: "anna" },
];

// ---- the simulated program ------------------------------------------------------------------

interface Vars {
  counter: number;
  name: string;
  origin: [number, number];
  n?: number;
  sum?: number;
  i?: number;
}
interface DStep {
  /** [function, line], top frame first. */
  frames: [string, number][];
  vars: Vars;
}

function trace(): DStep[] {
  const out: DStep[] = [];
  const v: Vars = { counter: 0, name: "", origin: [0, 0] };
  const main = (line: number) => out.push({ frames: [["main", line]], vars: { ...v } });
  const compute = (line: number) => out.push({ frames: [["compute", line], ["main", 17]], vars: { ...v } });
  main(14);
  main(15);
  v.name = "forge";
  main(16);
  v.origin = [3, 4];
  main(17);
  v.n = 4;
  compute(6);
  v.sum = 0;
  v.i = 0;
  compute(7);
  for (let i = 0; i < 4; i++) {
    v.i = i;
    compute(8);
    v.sum += i;
    v.i = i + 1;
    compute(7);
  }
  v.i = undefined;
  compute(10);
  const result = v.sum;
  v.n = undefined;
  v.sum = undefined;
  v.counter = result;
  main(18);
  v.counter += v.origin[0];
  main(19);
  main(20);
  return out;
}

const CODE: Record<number, string[]> = {
  5: ["push rbp", "mov rbp, rsp", "mov dword ptr [rbp - 0x14], edi"],
  6: ["mov dword ptr [rbp - 0x4], 0x0"],
  7: ["mov dword ptr [rbp - 0x8], 0x0", "mov eax, dword ptr [rbp - 0x8]", "cmp eax, dword ptr [rbp - 0x14]", "jge 0x401188"],
  8: ["mov eax, dword ptr [rbp - 0x8]", "add dword ptr [rbp - 0x4], eax", "add dword ptr [rbp - 0x8], 0x1"],
  10: ["mov eax, dword ptr [rbp - 0x4]", "pop rbp", "ret"],
  13: ["push rbp", "mov rbp, rsp", "sub rsp, 0x20"],
  14: ["mov dword ptr [rbp - 0x4], 0x0"],
  15: ["movabs rax, 0x6567726f66", "mov qword ptr [rbp - 0xc], rax"],
  16: ["mov dword ptr [rbp - 0x14], 0x3", "mov dword ptr [rbp - 0x10], 0x4"],
  17: ["mov edi, 0x4", "call 0x401126 <compute>", "mov dword ptr [rbp - 0x4], eax"],
  18: ["mov eax, dword ptr [rbp - 0x14]", "add dword ptr [rbp - 0x4], eax"],
  19: ["mov edx, dword ptr [rbp - 0x4]", "lea rax, [rbp - 0xc]", "mov rsi, rax", "call 0x401030 <printf@plt>"],
  20: ["mov eax, 0x0"],
  21: ["leave", "ret"],
};
const lineAddress = (line: number) => 0x401100 + line * 0x10;
const hex = (a: number, width = 16) => `0x${a.toString(16).padStart(width, "0")}`;
const COUNTER = 0x7ffef000;
const NAME = 0x7ffef008;
const ORIGIN = 0x7ffef010;
const N_ADDR = 0x7ffeefe0;
const SUM_ADDR = 0x7ffeefe4;
const I_ADDR = 0x7ffeefe8;
const LOCALS = 1000;
const ORIGIN_REF = 3000;

interface DapMockSession {
  info: SessionInfo;
  steps: DStep[];
  pc: number;
  ip: number;
  hits: Map<number, number>;
  writes: Map<number, number>;
  core: boolean;
}

const sessions = new Map<string, DapMockSession>();
let seq = 0;

const isDap = (args: Record<string, unknown>) => String(args.sessionId ?? "").startsWith("dap-");

function session(args: Record<string, unknown>): DapMockSession {
  const s = sessions.get(String(args.sessionId));
  if (!s) throw new Error(`no debug session ${String(args.sessionId)}`);
  return s;
}

const step = (s: DapMockSession) => s.steps[s.pc];
const topLine = (s: DapMockSession) => step(s).frames[0][1];
const currentAddress = (s: DapMockSession) => lineAddress(topLine(s)) + s.ip * 4;
const snapshot = () => call<BreakpointsSnapshot>("debug_breakpoints");

interface Local {
  name: string;
  value: string;
  type: string;
  address: number;
  num?: number;
  children?: boolean;
}

function locals(s: DapMockSession, level: number): Local[] {
  const v = step(s).vars;
  if (step(s).frames[level]?.[0] === "compute") {
    const out: Local[] = [
      { name: "n", value: String(v.n ?? 0), type: "int", address: N_ADDR, num: v.n },
      { name: "sum", value: String(v.sum ?? 0), type: "int", address: SUM_ADDR, num: v.sum },
    ];
    if (v.i !== undefined) out.push({ name: "i", value: String(v.i), type: "int", address: I_ADDR, num: v.i });
    return out;
  }
  return [
    { name: "counter", value: String(v.counter), type: "int", address: COUNTER, num: v.counter },
    { name: "name", value: `"${v.name}"`, type: "char[8]", address: NAME },
    { name: "origin", value: `{x=${v.origin[0]}, y=${v.origin[1]}}`, type: "struct point", address: ORIGIN, children: true },
  ];
}

function prop(l: Local, level: number, container: number, fullname = l.name): Property {
  return {
    name: l.name,
    fullname,
    type: "dap",
    classname: l.type,
    value: l.value,
    hasChildren: !!l.children,
    numChildren: l.children ? 2 : 0,
    page: 0,
    pageSize: 0,
    children: [],
    reference: l.children ? ORIGIN_REF + level : undefined,
    container,
    memoryReference: hex(l.address),
  };
}

function originChildren(s: DapMockSession, level: number): Property[] {
  const [x, y] = step(s).vars.origin;
  return [
    prop({ name: "x", value: String(x), type: "int", address: ORIGIN }, level, ORIGIN_REF + level, "origin.x"),
    prop({ name: "y", value: String(y), type: "int", address: ORIGIN + 4 }, level, ORIGIN_REF + level, "origin.y"),
  ];
}

function registers(s: DapMockSession, level: number): Property[] {
  const v = step(s).vars;
  const inCompute = step(s).frames[level]?.[0] === "compute";
  const rsp = inCompute ? 0x7ffeefd0 : 0x7ffeeff0;
  const rip = level === 0 ? currentAddress(s) : lineAddress(step(s).frames[level][1]) + 4;
  const reg = (name: string, value: string): Property => ({
    name,
    fullname: `$${name}`,
    type: "dap",
    classname: "unsigned long",
    value,
    hasChildren: false,
    numChildren: 0,
    page: 0,
    pageSize: 0,
    children: [],
  });
  return [
    reg("rax", hex(inCompute ? (v.sum ?? 0) : v.counter)),
    reg("rbx", hex(0)),
    reg("rcx", hex(0x7ffff7e1a2c0)),
    reg("rdx", hex(0x7ffeeff8)),
    reg("rsi", hex(NAME)),
    reg("rdi", hex(inCompute ? (v.n ?? 0) : 1)),
    reg("rsp", hex(rsp)),
    reg("rbp", hex(rsp + 0x20)),
    reg("rip", hex(rip)),
    reg("eflags", "0x00000246"),
  ];
}

function evaluate(s: DapMockSession, expression: string, level: number): Property {
  const e = expression.trim();
  const op = /^(.+?)\s*(==|!=|>=|<=|>|<|\+|-|\*)\s*(.+)$/.exec(e);
  if (op) {
    const l = Number(evaluate(s, op[1], level).value);
    const r = Number(evaluate(s, op[3], level).value);
    if (Number.isNaN(l) || Number.isNaN(r)) throw new Error("invalid operands to binary expression");
    const fns: Record<string, (a: number, b: number) => number> = {
      "==": (a, b) => +(a === b),
      "!=": (a, b) => +(a !== b),
      ">=": (a, b) => +(a >= b),
      "<=": (a, b) => +(a <= b),
      ">": (a, b) => +(a > b),
      "<": (a, b) => +(a < b),
      "+": (a, b) => a + b,
      "-": (a, b) => a - b,
      "*": (a, b) => a * b,
    };
    return { ...prop({ name: e, value: String(fns[op[2]](l, r)), type: "int", address: 0 }, level, 0, e), memoryReference: undefined, container: undefined };
  }
  if (/^-?\d+$/.test(e)) return { ...prop({ name: e, value: e, type: "int", address: 0 }, level, 0, e), memoryReference: undefined };
  const all = locals(s, level);
  const addr = /^&(\w+)$/.exec(e);
  if (addr) {
    const l = all.find((x) => x.name === addr[1]);
    if (!l) throw new Error(`use of undeclared identifier '${addr[1]}'`);
    return { ...prop({ name: e, value: hex(l.address), type: `${l.type} *`, address: l.address }, level, 0, e), container: undefined };
  }
  const [base, field] = e.split(".");
  const l = all.find((x) => x.name === base);
  if (!l) throw new Error(`use of undeclared identifier '${base}'`);
  if (field) {
    const child = base === "origin" ? originChildren(s, level).find((c) => c.name === field) : undefined;
    if (!child) throw new Error(`no member named '${field}' in '${l.type}'`);
    return { ...child, name: e, fullname: e, container: undefined };
  }
  return { ...prop(l, level, 0, e), container: undefined };
}

function interpolate(s: DapMockSession, template: string): string {
  return template.replace(/\{([^}]+)\}/g, (_, expr: string) => {
    try {
      return evaluate(s, expr, 0).value ?? "";
    } catch (e) {
      return `<${String(e).replace(/^Error: /, "")}>`;
    }
  });
}

/** Like `forge_dap::session::log_message`. */
function logMessage(bp: Breakpoint): string {
  const parts: string[] = [];
  const expr = bp.logExpression?.trim();
  if (bp.logMessage || !expr) parts.push(`Breakpoint reached at ${bp.file}:${bp.line}`);
  if (expr) parts.push(expr.includes("{") ? expr : `${expr} = {${expr}}`);
  return parts.join(" ");
}

function memoryByte(s: DapMockSession, addr: number): number | null {
  if (addr < 0x7ffeef00 || addr >= 0x7ffef100) return null;
  if (s.writes.has(addr)) return s.writes.get(addr)!;
  const v = step(s).vars;
  const int = (base: number, value: number | undefined) =>
    value !== undefined && addr >= base && addr < base + 4 ? (value >>> ((addr - base) * 8)) & 0xff : null;
  if (addr >= NAME && addr < NAME + 8) return v.name.charCodeAt(addr - NAME) || 0;
  return int(COUNTER, v.counter) ?? int(ORIGIN, v.origin[0]) ?? int(ORIGIN + 4, v.origin[1]) ?? int(N_ADDR, v.n) ?? int(SUM_ADDR, v.sum) ?? int(I_ADDR, v.i) ?? (addr % 16 === 8 ? 0x7f : 0);
}

// ---- execution ------------------------------------------------------------------------------

function features(): SessionInfo["features"] {
  return {
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
    goto: true,
    restart: true,
    completions: true,
    setVariable: true,
    modules: true,
    loadedSources: true,
    attached: false,
    exceptionFilters: [
      { filter: "cpp_throw", label: "C++: on throw", default: false, supportsCondition: true },
      { filter: "cpp_catch", label: "C++: on catch", default: false, supportsCondition: true },
    ],
  };
}

function output(s: DapMockSession, stream: string, text: string) {
  emit({ type: "output", sessionId: s.info.id, stream, text });
}

function stopped(s: DapMockSession, reason: SessionInfo["reason"], exception: SessionInfo["exception"] = null) {
  s.info = {
    ...s.info,
    status: "paused",
    reason,
    exception,
    threadId: 1,
    location: { file: FILE, fileUri: ABS, line: topLine(s) },
  };
  emit({ type: "session", session: s.info });
}

function end(s: DapMockSession, code: number) {
  output(s, "system", `\nProcess finished with exit code ${code}\n`);
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

function lineBreakpoints(): Breakpoint[] {
  const snap = snapshot();
  if (snap.muted) return [];
  return snap.breakpoints.filter((b) => b.enabled && b.kind === "line" && b.file === FILE);
}

/** The breakpoint that stops at the current line; logpoints log and run on. */
function hitAt(s: DapMockSession): Breakpoint | null {
  let stop: Breakpoint | null = null;
  for (const bp of lineBreakpoints().filter((b) => b.line === topLine(s))) {
    if (bp.condition?.trim()) {
      try {
        if (Number(evaluate(s, bp.condition, 0).value) === 0) continue;
      } catch {
        continue;
      }
    }
    const hits = (s.hits.get(bp.id) ?? 0) + 1;
    s.hits.set(bp.id, hits);
    if (bp.passCount && hits < bp.passCount) continue;
    if (bp.suspend === false) {
      output(s, "log", `${interpolate(s, logMessage(bp))}\n`);
      continue;
    }
    stop = bp;
  }
  return stop;
}

type Mode = "continue" | "next" | "stepIn" | "stepOut";

function run(s: DapMockSession, mode: Mode) {
  const startDepth = step(s).frames.length;
  const watched = snapshot().breakpoints.filter((b) => b.enabled && b.kind === "data" && b.adapter === "lldb");
  for (;;) {
    const prev = step(s);
    if (topLine(s) === 19) output(s, "stdout", `${prev.vars.name} ${prev.vars.counter}\n`);
    if (s.pc + 1 >= s.steps.length) return end(s, 0);
    s.pc += 1;
    s.ip = 0;
    const cur = step(s);
    for (const w of watched) {
      const key = (w.dataId ?? "").replace(/^&/, "") as keyof Vars;
      if (prev.vars[key] !== undefined && cur.vars[key] !== undefined && prev.vars[key] !== cur.vars[key]) {
        output(s, "log", `Watchpoint: ${w.description ?? key} changed from ${String(prev.vars[key])} to ${String(cur.vars[key])}\n`);
        return stopped(s, "breakpoint");
      }
    }
    const fn = snapshot().breakpoints.find((b) => b.enabled && b.kind === "function" && b.function === "compute");
    if (fn && cur.frames.length > prev.frames.length) return stopped(s, "breakpoint");
    const bp = hitAt(s);
    if (bp) {
      if (bp.logMessage || bp.logExpression?.trim()) output(s, "log", `${interpolate(s, logMessage(bp))}\n`);
      if (bp.temporary) call("debug_breakpoint_remove", { id: bp.id });
      return stopped(s, "breakpoint");
    }
    const depth = cur.frames.length;
    if (mode === "stepIn" || (mode === "next" && depth <= startDepth) || (mode === "stepOut" && depth < startDepth)) return stopped(s, "step");
  }
}

function resume(s: DapMockSession, fn: () => void) {
  s.info = { ...s.info, status: "running", location: null, reason: null, exception: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => sessions.has(s.info.id) && fn(), 120);
}

function start(request: LaunchRequest): SessionInfo {
  const id = `dap-${++seq}`;
  const adapter = ADAPTERS.find((a) => a.type === request.adapter) ?? ADAPTERS[0];
  const attached = request.mode === "attachProcess" || request.mode === "attachServer";
  const title =
    request.name ??
    (request.mode === "attachProcess" ? `demo (${request.pid})` : request.mode === "coreDump" ? (request.core.split("/").pop() ?? "core") : "demo");
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "c",
    engine: adapter.label,
    fileUri: PROGRAM,
    file: "scripts/native/demo",
    peer: adapter.command ?? adapter.type,
    serverName: title,
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: adapter.type,
    features: { ...features()!, attached },
  };
  const s: DapMockSession = { info, steps: trace(), pc: 0, ip: 0, hits: new Map(), writes: new Map(), core: request.mode === "coreDump" };
  sessions.set(id, s);
  // The adapter's exception filters become (disabled) breakpoints, like in Rust.
  for (const f of info.features!.exceptionFilters) {
    const exists = snapshot().breakpoints.some((b) => b.kind === "exception" && b.adapter === adapter.type && b.exception === f.filter);
    if (!exists) call("debug_breakpoint_add", { spec: { kind: "exception", exception: f.filter, description: f.label, adapter: adapter.type, enabled: f.default } satisfies BreakpointSpec });
  }
  emit({ type: "session", session: info });
  output(s, "system", `${PROGRAM.split("/").pop()} (pid ${request.mode === "attachProcess" ? request.pid : 4242})\n`);
  setTimeout(() => {
    if (!sessions.has(id)) return;
    if (s.core) {
      s.pc = s.steps.findIndex((x) => x.frames[0][1] === 19);
      return stopped(s, "exception", { name: "SIGSEGV", message: "Core dump: segmentation fault" });
    }
    if (request.stopOnEntry) return stopped(s, "firstLine");
    const first = hitAt(s);
    if (first) return stopped(s, "breakpoint");
    run(s, "continue");
  }, 300);
  return info;
}

// ---- handlers -------------------------------------------------------------------------------

registerMock("dap_adapters", () => ADAPTERS);
registerMock("dap_processes", (args) => {
  const f = String(args.filter ?? "").toLowerCase();
  return PROCESSES.filter((p) => !f || p.name.toLowerCase().includes(f) || p.commandLine.toLowerCase().includes(f) || String(p.pid).includes(f));
});
registerMock("dap_start", (args) => start(args.request as LaunchRequest));
registerMock("dap_restart", (args) => {
  const s = session(args);
  s.pc = 0;
  s.ip = 0;
  s.hits.clear();
  resume(s, () => (hitAt(s) ? stopped(s, "breakpoint") : run(s, "continue")));
  return s.info;
});
registerMock("dap_threads", () => [
  { id: 1, name: "demo" },
  { id: 2, name: "worker" },
]);
registerMock("dap_goto", (args) => {
  const s = session(args);
  const line = Number(args.line);
  const depth = step(s).frames.length;
  const func = step(s).frames[0][0];
  const target = s.steps.findIndex((x) => x.frames.length === depth && x.frames[0][0] === func && x.frames[0][1] === line);
  if (target < 0) throw new Error(`Cannot jump to line ${line}`);
  resume(s, () => {
    s.pc = target;
    s.ip = 0;
    stopped(s, "step");
  });
});
registerMock("dap_registers", (args) => registers(session(args), Number(args.depth ?? 0)));
registerMock("dap_read_memory", (args) => {
  const s = session(args);
  const base = Number.parseInt(String(args.memoryReference).replace(/^0x/, ""), 16) + Number(args.offset ?? 0);
  const count = Number(args.count ?? 256);
  const rows: MemoryRow[] = [];
  let unreadable = 0;
  for (let r = 0; r < count; r += 16) {
    const hexes: string[] = [];
    let ascii = "";
    for (let c = 0; c < 16 && r + c < count; c++) {
      const b = memoryByte(s, base + r + c);
      if (b === null) {
        unreadable++;
        hexes.push("??");
        ascii += " ";
      } else {
        hexes.push(b.toString(16).padStart(2, "0"));
        ascii += b >= 32 && b < 127 ? String.fromCharCode(b) : ".";
      }
    }
    rows.push({ address: hex(base + r, 8), hex: hexes, ascii });
  }
  return { address: hex(base, 8), rows, unreadable };
});
registerMock("dap_write_memory", (args) => {
  const s = session(args);
  const base = Number.parseInt(String(args.memoryReference).replace(/^0x/, ""), 16) + Number(args.offset ?? 0);
  const bytes = String(args.data).trim().split(/[\s,]+/).map((b) => Number.parseInt(b.replace(/^0x/, ""), 16));
  bytes.forEach((b, n) => s.writes.set(base + n, b));
  emit({ type: "invalidated", sessionId: s.info.id, areas: ["memory", "variables"] });
});
registerMock("dap_disassemble", (args) => {
  const all: { address: number; line: number; text: string }[] = [];
  for (const [line, code] of Object.entries(CODE)) code.forEach((text, k) => all.push({ address: lineAddress(Number(line)) + k * 4, line: Number(line), text }));
  const start = Number.parseInt(String(args.memoryReference).replace(/^0x/, ""), 16);
  const current = args.current ? Number.parseInt(String(args.current).replace(/^0x/, ""), 16) : -1;
  const anchor = Math.max(0, all.findIndex((i) => i.address >= start));
  const from = Math.max(0, anchor + Number(args.instructionOffset ?? 0));
  return all.slice(from, from + Number(args.count ?? 40)).map(
    (i): Instruction => ({
      address: hex(i.address),
      bytes: `${(i.address & 0xff).toString(16).padStart(2, "0")} ${i.line.toString(16).padStart(2, "0")} ${(i.text.length & 0xff).toString(16).padStart(2, "0")} 90`,
      instruction: i.text,
      symbol: `${i.line >= 13 ? "main" : "compute"}+${i.address - lineAddress(i.line >= 13 ? 13 : 5)}`,
      file: FILE,
      line: i.line,
      current: i.address === current,
    }),
  );
});
registerMock("dap_modules", () => [
  { id: 1, name: "demo", path: PROGRAM, symbolStatus: "Symbols loaded.", addressRange: "0x0000000000401000" },
  { id: 2, name: "libc.so.6", path: "/usr/lib/x86_64-linux-gnu/libc.so.6", symbolStatus: "Symbols not found." },
]);
registerMock("dap_loaded_sources", () => [{ name: "demo.c", path: ABS, file: FILE }]);
registerMock("dap_data_breakpoint", (args) => {
  const name = String(args.name);
  if (!["counter", "n", "sum", "i"].includes(name)) throw new Error(`'${name}' is not a scalar local variable`);
  return call<Breakpoint>("debug_breakpoint_add", { spec: { kind: "data", dataId: `&${name}`, description: name, accessType: "write", adapter: "lldb" } satisfies BreakpointSpec });
});

/** Route `debug_*` commands of DAP sessions here, the rest to the Xdebug mock. */
function route(cmd: string, fn: (args: Record<string, unknown>) => unknown) {
  const xdebug = mockHandler(cmd)!;
  registerMock(cmd, (args) => (isDap(args) ? fn(args) : xdebug(args)));
}

const withDap = (cmd: string) => {
  const xdebug = mockHandler(cmd)!;
  registerMock(cmd, async (args) => {
    const base = (await xdebug(args)) as SessionInfo[] | { sessions: SessionInfo[] };
    const dap = [...sessions.values()].map((s) => s.info);
    return Array.isArray(base) ? [...base, ...dap] : { ...base, sessions: [...base.sessions, ...dap] };
  });
};
withDap("debug_status");
withDap("debug_sessions");

route("debug_step", (args) => {
  const s = session(args);
  if (s.info.status !== "paused") throw new Error("the program is not suspended");
  if (s.core) throw new Error("a core dump cannot run");
  const kind = args.kind as StepKind;
  if (kind === "stepIntoInstruction" || kind === "stepOverInstruction") {
    return resume(s, () => {
      if (s.ip + 1 < (CODE[topLine(s)]?.length ?? 1)) {
        s.ip += 1;
        stopped(s, "step");
      } else run(s, "stepIn");
    });
  }
  const mode: Mode = kind === "run" ? "continue" : kind === "stepOver" ? "next" : kind === "stepOut" ? "stepOut" : "stepIn";
  resume(s, () => run(s, mode));
});
route("debug_pause", () => undefined);
route("debug_stop", (args) => end(session(args), 143));
route("debug_detach", (args) => end(session(args), 0));
route("debug_run_to_cursor", (args) => {
  const s = session(args);
  const line = Number(args.line);
  resume(s, () => {
    const target = s.steps.findIndex((x, n) => n > s.pc && x.frames[0][1] === line);
    if (target < 0) return run(s, "continue");
    s.pc = target;
    stopped(s, "breakpoint");
  });
});
route("debug_stack", (args): Frame[] => {
  const s = session(args);
  if (Number(args.threadId ?? 1) === 2) {
    return [
      { level: 0, file: null, fileUri: "", line: 0, where: "__futex_abstimed_wait_common", id: 51, address: "0x00007ffff7c91117" },
      { level: 1, file: null, fileUri: "", line: 0, where: "worker_loop", id: 52, address: "0x00007ffff7c9a2c4" },
      { level: 2, file: null, fileUri: "", line: 0, where: "start_thread", id: 53, address: "0x00007ffff7c94ac3" },
    ];
  }
  return step(s).frames.map(([where, line], level) => ({
    level,
    file: FILE,
    fileUri: ABS,
    line,
    where,
    id: level + 1,
    address: hex(level === 0 ? currentAddress(s) : lineAddress(line) + 4),
  }));
});
route("debug_contexts", () => [
  { id: 0, name: "Locals" },
  { id: 1, name: "Registers" },
]);
route("debug_variables", (args) => {
  const s = session(args);
  const level = Number(args.depth ?? 0);
  if (Number(args.context ?? 0) === 1) return registers(s, level);
  return locals(s, level).map((l) => prop(l, level, LOCALS + level));
});
route("debug_property", (args) => {
  const s = session(args);
  const reference = Number(args.reference);
  const level = reference - ORIGIN_REF;
  const children = level >= 0 && level < 10 ? originChildren(s, level) : [];
  return { name: String(args.fullname), fullname: String(args.fullname), type: "dap", hasChildren: true, numChildren: children.length, page: 0, pageSize: 0, children, reference };
});
route("debug_eval", (args) => evaluate(session(args), String(args.expression), Number(args.depth ?? 0)));
route("debug_set_value", (args) => {
  const s = session(args);
  const name = String(args.name ?? args.fullname);
  const value = Number(String(args.value).trim());
  if (!Number.isInteger(value)) throw new Error("expected an integer");
  const v = step(s).vars;
  if (name === "counter") {
    const delta = value - v.counter;
    for (const later of s.steps.slice(s.pc)) later.vars = { ...later.vars, counter: later.vars.counter + delta };
  } else if (name === "sum" && v.sum !== undefined) step(s).vars = { ...v, sum: value };
  else if (name === "n" && v.n !== undefined) step(s).vars = { ...v, n: value };
  else throw new Error(`cannot set '${name}'`);
});
route("debug_complete", (args) => {
  const s = session(args);
  const prefix = /[A-Za-z_]\w*$/.exec(String(args.text))?.[0] ?? "";
  return locals(s, Number(args.depth ?? 0))
    .filter((l) => l.name.startsWith(prefix))
    .map((l) => ({ label: l.name, insert: l.name, detail: l.type }));
});

// Screenshot scenarios can start sessions directly.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeDapMock?: unknown }).__forgeDapMock = { start };
}
