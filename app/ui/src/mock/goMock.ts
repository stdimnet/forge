/**
 * Browser-mode GoForge for the `ledger` demo (`?project=go`): the SDK and tools of this
 * "machine", SDK downloads (canned steps), go.mod / go.work analysis, completion contexts,
 * module and go generate command lines, go vet / golangci-lint reports and pprof views —
 * computed by the Rust crates through WebAssembly; canned `go test -json` runs (a failing
 * subtest included) and a simulated Delve session that stops in `report.Total`. Nothing
 * runs and nothing is written.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { Breakpoint, BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { AdapterInfo, LaunchRequest } from "@/features/debug/dapApi";
import type { GoDownloadable, GoStatus, ModCompletionResult } from "@/ipc/go";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { callWasm, forgeWasm } from "@/wasm";
import { GO_MODULE, goDemoInfo, goDemoName, isGoDemo } from "./demoGoProject";
import { goContext, MODULES, SDKS } from "./goTestsMock";
import { onMockDidOpen, setMockServerStatus } from "./lspMock";
import { mockSettingValue, setSetting } from "./settingsMock";

const ROOT = `~/Projects/${goDemoName}`;
const ABS = `/home/anna/Projects/${goDemoName}`;
const call = async <T>(fn: string, input: unknown): Promise<T> => {
  const wasm = await forgeWasm();
  return callWasm<T>((wasm as unknown as Record<string, (json: string) => string>)[fn], input);
};

function status(): GoStatus {
  const custom = String(mockSettingValue("go.goroot") ?? "");
  const sdk = custom ? (SDKS.find((s) => s.root === custom) ?? { root: custom, version: "", source: "custom" }) : SDKS[0];
  return {
    sdk,
    custom: !!custom,
    sdks: SDKS,
    modules: MODULES,
    gopath: String(mockSettingValue("go.gopath") ?? "") || "/home/anna/go",
    tools: [
      { tool: "gopls", label: "gopls", path: "/home/anna/go/bin/gopls", install: "golang.org/x/tools/gopls@v0.20.0", licence: "BSD-3-Clause" },
      { tool: "dlv", label: "Delve", path: "/home/anna/.local/share/forge/go-tools/bin/dlv", install: "github.com/go-delve/delve/cmd/dlv@v1.25.2", licence: "MIT" },
      { tool: "goimports", label: "goimports", path: null, install: "golang.org/x/tools/cmd/goimports@v0.30.0", licence: "BSD-3-Clause" },
      { tool: "golangciLint", label: "golangci-lint", path: "/usr/local/bin/golangci-lint", install: null, licence: "GPL-3.0" },
    ],
  };
}

const RELEASES: GoDownloadable[] = ["1.25.3", "1.24.9", "1.23.12"].map((version, i) => ({
  version,
  file: {
    filename: `go${version}.linux-amd64.tar.gz`,
    os: "linux",
    arch: "amd64",
    version: `go${version}`,
    sha256: ["0335f314b6e7bfe08c3d0cfaa7c19db961b7b99fb20be62b0a826c992ad14e0f", "e1c1b6b5a4f8a5d2b1e6f0a4a1c3e9f0d2b7a8c6e5f4d3c2b1a0f9e8d7c6b5a4", "5b2c1a0f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b"][i],
    size: [75_436_812, 75_102_365, 73_965_408][i],
    kind: "archive",
  },
}));

registerMock("go_status", () => status());
registerMock("go_sdk_releases", () => RELEASES);
registerMock("go_sdk_download", async ({ version }) => {
  const d = RELEASES.find((r) => r.version === String(version)) ?? RELEASES[0];
  const steps = [`Downloading ${d.file.filename}…`, `Verified SHA-256 of ${d.file.filename}.`, "Unpacking the Go SDK…"];
  for (const message of steps) emitMock("go://install", { message });
  const sdk = { root: `/home/anna/.local/share/forge/go-sdks/go${d.version}`, version: d.version, source: "Forge" };
  if (!SDKS.some((s) => s.root === sdk.root)) SDKS.unshift(sdk);
  emitMock("go://install", { message: `Go SDK ready in ${sdk.root}.` });
  return sdk;
});
registerMock("go_tool_install_command", ({ tool }): CommandLine => {
  const target = status().tools.find((t) => t.tool === tool)?.install;
  if (!target) throw new Error("Forge does not install golangci-lint; see its website");
  return { program: "/usr/local/go/bin/go", args: ["install", target], cwd: "/tmp", env: { GOBIN: "/home/anna/.local/share/forge/go-tools/bin", GOTOOLCHAIN: "auto" } };
});

// ---- go.mod / go.work -------------------------------------------------------------------

registerMock("go_mod_parse", ({ path, content }) => call("go_mod_parse", { path, content }));

const KNOWN_MODULES = [
  "github.com/google/uuid",
  "github.com/stretchr/testify",
  "github.com/spf13/cobra",
  "github.com/jackc/pgx/v5",
  "golang.org/x/text",
  "golang.org/x/sync",
  "golang.org/x/exp",
  "gopkg.in/yaml.v3",
];
const VERSIONS: Record<string, string[]> = {
  "github.com/google/uuid": ["v1.6.0", "v1.5.0", "v1.4.0", "v1.3.1"],
  "github.com/stretchr/testify": ["v1.10.0", "v1.9.0", "v1.8.4"],
  "github.com/spf13/cobra": ["v1.9.1", "v1.8.1", "v1.8.0"],
  "golang.org/x/text": ["v0.30.0", "v0.29.0", "v0.21.0", "v0.20.0"],
  "golang.org/x/sync": ["v0.17.0", "v0.16.0", "v0.10.0"],
};
const versionsOf = (m: string) => VERSIONS[m] ?? ["v1.0.0"];

registerMock("go_module_versions", ({ module }) => versionsOf(String(module)));
registerMock("go_mod_complete", async ({ path, content, offset }): Promise<ModCompletionResult | null> => {
  type Ctx = { kind: "directive" | "modulePath" | "version" | "goVersion"; from: number; prefix: string; module?: string };
  const ctx = await call<Ctx | null>("go_mod_completion_context", { path, content, offset });
  if (!ctx) return null;
  switch (ctx.kind) {
    case "directive": {
      const list = String(path).endsWith("go.work") ? ["go", "toolchain", "godebug", "use", "replace"] : ["module", "go", "toolchain", "godebug", "require", "replace", "exclude", "retract", "tool", "ignore"];
      return { from: ctx.from, items: list.map((label) => ({ label, kind: "directive" })) };
    }
    case "goVersion":
      return { from: ctx.from, items: SDKS.map((s) => ({ label: s.version, detail: "installed SDK", kind: "version" })) };
    case "modulePath":
      return { from: ctx.from, items: KNOWN_MODULES.filter((m) => m.startsWith(ctx.prefix)).map((label) => ({ label, detail: "module cache", kind: "module" })) };
    default:
      return { from: ctx.from, items: versionsOf(ctx.module ?? "").map((label, i) => ({ label, detail: i === 0 ? "latest" : null, kind: "version" })) };
  }
});

const toCommandLine = (c: { program: string; args: string[]; cwd: string; env: Record<string, string> }): CommandLine => ({
  program: c.program,
  args: c.args,
  cwd: c.cwd,
  env: c.env,
});

registerMock("go_module_command", async ({ action, arg, dir }) =>
  toCommandLine(await call("go_module_command", { action, arg: arg ?? "", dir: dir ?? "", context: goContext() })),
);

// ---- go generate --------------------------------------------------------------------------

registerMock("go_generate_directives", ({ content }) => call("go_generate_directives", { path: "", content }));
registerMock("go_generate_command", async ({ path, content, line }): Promise<CommandLine> => ({
  program: `${goContext().goroot}/bin/go`,
  args: await call<string[]>("go_generate_args", { path, content, line: line ?? null }),
  cwd: "",
  env: {},
}));

// ---- gofmt, go vet, golangci-lint -----------------------------------------------------------

/** gofmt of the demo: indentation with tabs, no trailing spaces. */
registerMock("go_format", ({ content }) =>
  String(content)
    .split("\n")
    .map((l) => l.replace(/^( {4})+/, (s) => "\t".repeat(s.length / 4)).replace(/[ \t]+$/, ""))
    .join("\n"),
);

const LINT_ISSUES = [
  { FromLinter: "errcheck", Text: "Error return value of `w.Write` is not checked", Pos: { Filename: "internal/store/store.go", Line: 45, Column: 10 } },
  { FromLinter: "errcheck", Text: "Error return value of `f.Close` is not checked", Pos: { Filename: "internal/store/store.go", Line: 24, Column: 8 } },
  { FromLinter: "govet", Text: "printf: fmt.Sprintf format %v has arg kindOf(e.Amount) of wrong type report.Kind", Pos: { Filename: "internal/report/report.go", Line: 55, Column: 9 } },
];
const VET = `# ${GO_MODULE}/internal/report\n{\n\t"${GO_MODULE}/internal/report": {\n\t\t"printf": [\n\t\t\t{\n\t\t\t\t"posn": "${ABS}/internal/report/report.go:55:9",\n\t\t\t\t"message": "fmt.Sprintf format %v has arg kindOf(e.Amount) of wrong type report.Kind; did you mean to run stringer?"\n\t\t\t}\n\t\t]\n\t}\n}\n`;

registerMock("go_lint", async ({ tool, path }) => {
  const dir = typeof path === "string" ? path.replace(/\/[^/]*$/, "") : "";
  const all =
    tool === "vet"
      ? await call<{ path: string }[]>("go_lint_parse", { tool: "vet", output: VET, root: ABS })
      : await call<{ path: string }[]>("go_lint_parse", { tool: "golangci", output: JSON.stringify({ Issues: LINT_ISSUES, Report: {} }), root: ABS });
  return dir ? all.filter((p) => p.path.startsWith(`${dir}/`)) : all;
});

// ---- Profiler ---------------------------------------------------------------------------------

registerMock("go_profile_path", ({ name }) => `/home/anna/.cache/forge/go-profiles/${String(name).replace(/[^\w-]/g, "_")}-1761480000.pprof`);
registerMock("pprof_open", ({ sampleIndex }) => call("pprof_view", { sampleIndex: sampleIndex ?? null }));

// ---- Delve ----------------------------------------------------------------------------------------

const REPORT = "internal/report/report.go";
const MONEY = "internal/money/money.go";
const MAIN = "cmd/ledger/main.go";

interface GoStep {
  frames: [string, string | null, number][];
  locals: Record<string, string>;
}

const ENTRIES = "[]github.com/acme/ledger/internal/store.Entry len: 2, cap: 2, [{…},{…}]";
const ENTRY_RENT = 'github.com/acme/ledger/internal/store.Entry {Date: "2026-10-01", Text: "Rent", Amount: -120000}';
const ENTRY_INVOICE = 'github.com/acme/ledger/internal/store.Entry {Date: "2026-10-03", Text: "Invoice 1042", Amount: 349900}';
const outer: [string, string | null, number][] = [
  ["github.com/acme/ledger/internal/report.Render", REPORT, 37],
  ["main.main", MAIN, 23],
  ["runtime.main", null, 283],
];
const TRACE: GoStep[] = [
  { frames: [["github.com/acme/ledger/internal/report.Total", REPORT, 19], ...outer], locals: { entries: ENTRIES, month: '"2026-10"', "~r0": "0" } },
  { frames: [["github.com/acme/ledger/internal/report.Total", REPORT, 20], ...outer], locals: { entries: ENTRIES, month: '"2026-10"', total: "0" } },
  { frames: [["github.com/acme/ledger/internal/report.Total", REPORT, 22], ...outer], locals: { entries: ENTRIES, month: '"2026-10"', total: "0", e: ENTRY_RENT } },
  { frames: [["github.com/acme/ledger/internal/report.Total", REPORT, 22], ...outer], locals: { entries: ENTRIES, month: '"2026-10"', total: "-120000", e: ENTRY_INVOICE } },
  { frames: [["github.com/acme/ledger/internal/report.Total", REPORT, 25], ...outer], locals: { entries: ENTRIES, month: '"2026-10"', total: "229900" } },
  { frames: [["github.com/acme/ledger/internal/money.Round", MONEY, 29], ["github.com/acme/ledger/internal/report.Total", REPORT, 25], ...outer], locals: { a: "229900" } },
];

const ADAPTER: AdapterInfo = { type: "go", label: "Go (Delve)", languages: ["go"], launch: true, attachProcess: true, attachPort: false, coreDump: true, command: "/home/anna/.local/share/forge/go-tools/bin/dlv" };

interface GoSession {
  info: SessionInfo;
  pc: number;
}

const sessions = new Map<string, GoSession>();
let seq = 700;
const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const output = (id: string, stream: string, text: string) => emit({ type: "output", sessionId: id, stream, text });
const top = (s: GoSession) => TRACE[s.pc].frames[0];

function breakpointAt(s: GoSession): Breakpoint | undefined {
  const snap = mockHandler("debug_breakpoints")!({}) as BreakpointsSnapshot;
  if (snap.muted) return undefined;
  const [, file, line] = top(s);
  return snap.breakpoints.find((b) => b.enabled && b.kind === "line" && b.file === file && b.line === line);
}

function stopped(s: GoSession, reason: SessionInfo["reason"]) {
  const [, file, line] = top(s);
  s.info = { ...s.info, status: "paused", reason, threadId: 1, location: { file, fileUri: `${ROOT}/${file}`, line } };
  emit({ type: "session", session: s.info });
}

function finish(s: GoSession) {
  output(s.info.id, "stdout", "Ledger 2026-10\n2026-10-01  Rent                       -1200.00  1\n2026-10-03  Invoice 1042                3499.00  0\nTotal 2299.00\n");
  output(s.info.id, "console", "Process 48211 has exited with status 0\n");
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

function run(s: GoSession, mode: "continue" | "next" | "stepIn" | "stepOut") {
  const depth = (x: number) => TRACE[x].frames.length;
  const start = depth(s.pc);
  for (;;) {
    if (s.pc + 1 >= TRACE.length) return finish(s);
    s.pc += 1;
    if (breakpointAt(s)) return stopped(s, "breakpoint");
    const d = depth(s.pc);
    if (mode === "stepIn" || (mode === "next" && d <= start) || (mode === "stepOut" && d < start)) return stopped(s, "step");
  }
}

function resume(s: GoSession, fn: () => void) {
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => sessions.has(s.info.id) && fn(), 120);
}

function start(request: LaunchRequest): SessionInfo {
  const id = `dap-${++seq}`;
  const title = request.name ?? "ledger";
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "go",
    engine: ADAPTER.label,
    fileUri: `${ROOT}/${MAIN}`,
    file: MAIN,
    peer: "127.0.0.1:38417",
    serverName: title,
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: "go",
    features: {
      threads: true,
      readMemory: true,
      writeMemory: false,
      disassemble: true,
      registers: false,
      dataBreakpoints: false,
      functionBreakpoints: true,
      conditionalBreakpoints: true,
      hitConditionalBreakpoints: true,
      logPoints: true,
      instructionStepping: true,
      goto: false,
      restart: false,
      completions: false,
      setVariable: true,
      modules: false,
      loadedSources: false,
      attached: request.mode !== "launch",
      exceptionFilters: [],
    },
  };
  const s: GoSession = { info, pc: 0 };
  sessions.set(id, s);
  emit({ type: "session", session: info });
  output(id, "console", `Starting: ${ADAPTER.command} dap --listen=127.0.0.1:38417\nDAP server listening at: 127.0.0.1:38417\n`);
  setTimeout(() => {
    if (!sessions.has(id)) return;
    output(id, "console", `Type 'dlv help' for list of commands.\n`);
    if (breakpointAt(s)) stopped(s, "breakpoint");
    else run(s, "continue");
  }, 500);
  return info;
}

const prop = (name: string, value: string, reference?: number): Property => ({
  name,
  fullname: name,
  type: "dap",
  classname: value.startsWith("[]") ? "[]store.Entry" : value.includes("store.Entry") ? "store.Entry" : value.startsWith('"') ? "string" : name === "total" || name === "a" ? "money.Amount" : "int",
  value,
  hasChildren: reference !== undefined,
  numChildren: reference !== undefined ? 3 : 0,
  page: 0,
  pageSize: 0,
  children: [],
  reference,
});

const own = (args: Record<string, unknown>) => sessions.get(String(args.sessionId ?? ""));

function route(cmd: string, fn: (s: GoSession, args: Record<string, unknown>) => unknown) {
  const previous = mockHandler(cmd);
  registerMock(cmd, (args) => {
    const s = own(args);
    if (s) return fn(s, args);
    return previous ? previous(args) : undefined;
  });
}

{
  const adapters = mockHandler("dap_adapters");
  registerMock("dap_adapters", async (args) => [...(((await adapters?.(args)) as AdapterInfo[] | undefined) ?? []), ADAPTER]);
  const dapStart = mockHandler("dap_start");
  registerMock("dap_start", (args) => {
    const request = args.request as LaunchRequest;
    return request.adapter === "go" ? start(request) : dapStart?.(args);
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

route("dap_threads", () => [
  { id: 1, name: "Goroutine 1 main.main" },
  { id: 2, name: "Goroutine 2 runtime.forcegchelper" },
  { id: 3, name: "Goroutine 3 runtime.bgsweep" },
  { id: 4, name: "Goroutine 4 runtime.bgscavenge" },
]);
route("debug_step", (s, args) => {
  if (s.info.status !== "paused") throw new Error("the program is not suspended");
  const kind = String(args.kind);
  const mode = kind === "run" ? "continue" : kind === "stepOver" ? "next" : kind === "stepOut" ? "stepOut" : "stepIn";
  resume(s, () => run(s, mode));
});
route("debug_pause", (s) => (s.info.status === "running" ? stopped(s, "pause") : undefined));
route("debug_stop", (s) => finish(s));
route("debug_detach", (s) => finish(s));
route("debug_stack", (s): Frame[] =>
  s.info.status !== "paused"
    ? []
    : TRACE[s.pc].frames.map(([where, file, line], level) => ({
        level,
        file,
        fileUri: file ? `${ROOT}/${file}` : "/usr/local/go/src/runtime/proc.go",
        line,
        where,
        id: level + 1,
      })),
);
route("debug_contexts", () => [
  { id: 0, name: "Locals" },
  { id: 1, name: "Globals" },
]);
const ENTRY_REF = 7101;
route("debug_variables", (s, args) => {
  const level = Number(args.depth ?? 0);
  if (Number(args.context ?? 0) === 1) return [prop("report.Income", "0"), prop("report.Expense", "1")];
  if (level > 0) {
    const [fn] = TRACE[s.pc].frames[level] ?? [];
    if (fn?.endsWith("report.Total")) return [prop("entries", ENTRIES, ENTRY_REF + 1), prop("month", '"2026-10"'), prop("total", "229900")];
    if (fn?.endsWith("report.Render")) return [prop("entries", ENTRIES, ENTRY_REF + 1), prop("month", '"2026-10"'), prop("lines", '[]string len: 3, cap: 4, ["Ledger 2026-10",…]')];
    if (fn === "main.main") return [prop("path", '*string "ledger.csv"'), prop("month", '*string "2026-10"')];
    return [];
  }
  return Object.entries(TRACE[s.pc].locals).map(([k, v]) => prop(k, v, k === "e" ? ENTRY_REF : k === "entries" ? ENTRY_REF + 1 : undefined));
});
route("debug_property", (s, args) => {
  const ref = Number(args.reference);
  const e = TRACE[s.pc].locals.e ?? ENTRY_RENT;
  const children =
    ref === ENTRY_REF
      ? e === ENTRY_RENT
        ? [prop("Date", '"2026-10-01"'), prop("Text", '"Rent"'), prop("Amount", "-120000")]
        : [prop("Date", '"2026-10-03"'), prop("Text", '"Invoice 1042"'), prop("Amount", "349900")]
      : [prop("[0]", ENTRY_RENT), prop("[1]", ENTRY_INVOICE)];
  return { ...prop(String(args.fullname), "", ref), hasChildren: true, numChildren: children.length, children: children.map((c) => ({ ...c, fullname: `${String(args.fullname)}.${c.name}` })) };
});
route("debug_eval", (s, args) => {
  const e = String(args.expression).trim();
  const locals = TRACE[s.pc].locals;
  if (e in locals) return prop(e, locals[e]);
  if (e === "len(entries)") return prop(e, "2");
  if (/^-?\d+$/.test(e)) return prop(e, e);
  throw new Error(`could not find symbol value for ${e}`);
});
route("debug_set_value", () => undefined);

registerMock("go_delve_status", () => ({ path: ADAPTER.command, install: "github.com/go-delve/delve/cmd/dlv@v1.25.2" }));
registerMock("go_debug_request", ({ configuration }): LaunchRequest => {
  const cfg = configuration as RunConfiguration;
  const o = cfg.options ?? {};
  if (cfg.type === "go-remote")
    return { mode: "attachServer", adapter: "go", name: cfg.name, host: o.host || "localhost", port: Number(o.port || 2345), configuration: o.remotePath ? { substitutePath: [{ from: ABS, to: o.remotePath }] } : undefined };
  if (cfg.type === "go-test")
    return { mode: "launch", adapter: "go", name: cfg.name, program: `${ABS}/${o.directory || "internal/money"}`, args: [`-test.run=^${o.test ?? ""}$`, "-test.v"], configuration: { mode: "test" } };
  return { mode: "launch", adapter: "go", name: cfg.name, program: `${ABS}/${(o.package ?? "./cmd/ledger").replace(/^\.\//, "")}`, arguments: cfg.arguments, configuration: { mode: "debug" } };
});

// ---- the demo project ----------------------------------------------------------------------------

if (isGoDemo()) {
  registerMock("project_detect", () => structuredClone(goDemoInfo));
  setSetting("project.announcedFrameworks", [`${ROOT}#go`], "user");
  onMockDidOpen((path) => {
    if (!/\.go$|go\.mod$|go\.work$/.test(path)) return;
    setMockServerStatus({ id: "gopls", name: "gopls", state: "ready" });
  });
}

// Screenshot scenarios can start sessions directly.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeGoDebugMock?: unknown }).__forgeGoDebugMock = { start };
}
