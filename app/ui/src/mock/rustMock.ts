/**
 * Browser-mode RustForge for the `shop` demo (`?project=rust`): the toolchains and tools of
 * this "machine", the Cargo workspace, Cargo command lines, `fn main` run targets, test
 * discovery, Clippy problems with fixes — computed by the Rust crates through WebAssembly —
 * canned `cargo run` / `cargo test` output, and a simulated LLDB session (rustc's pretty
 * printers: `Vec`, `String`, `Option`, `HashMap` as values) that stops in `main`. Nothing
 * runs and nothing is written.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { Breakpoint, BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { AdapterInfo, LaunchRequest } from "@/features/debug/dapApi";
import type { CargoWorkspace, RustStatus } from "@/ipc/rust";
import type { RunConfiguration } from "@/ipc/run";
import { callWasm, forgeWasm } from "@/wasm";
import { isRustDemo, rustDemoInfo, rustDemoName } from "./demoRustProject";
import { onMockDidOpen, setMockServerStatus } from "./lspMock";
import { RUST_ROOT, rustContext } from "./rustTestsMock";
import { mockSettingValue, setSetting } from "./settingsMock";

const ROOT = `~/Projects/${rustDemoName}`;
const call = async <T>(fn: string, input: unknown): Promise<T> => {
  const wasm = await forgeWasm();
  return callWasm<T>((wasm as unknown as Record<string, (json: string) => string>)[fn], input);
};

const RUSTUP = "/home/anna/.rustup/toolchains";
const STABLE = "stable-x86_64-unknown-linux-gnu";
const SYSROOT = `${RUSTUP}/${STABLE}`;

function status(): RustStatus {
  const chosen = String(mockSettingValue("rust.toolchain") ?? "");
  const active = chosen ? `${chosen}-x86_64-unknown-linux-gnu` : STABLE;
  const toolchain = (name: string, channel: string, isDefault = false) => ({
    name,
    channel,
    host: "x86_64-unknown-linux-gnu",
    path: `${RUSTUP}/${name}`,
    default: isDefault,
    active: name === active,
    source: "rustup",
  });
  return {
    rustup: "/home/anna/.cargo/bin/rustup",
    cargo: "/home/anna/.cargo/bin/cargo",
    toolchains: [
      toolchain(STABLE, "stable", true),
      toolchain("nightly-x86_64-unknown-linux-gnu", "nightly"),
      toolchain("1.85.0-x86_64-unknown-linux-gnu", "1.85.0"),
    ],
    active: chosen
      ? { name: active, reason: { kind: "other", text: "chosen in Settings | Rust" } }
      : { name: STABLE, reason: { kind: "file", path: `${RUST_ROOT}/rust-toolchain.toml` } },
    toolchainFile: { path: "rust-toolchain.toml", file: { channel: "stable", components: ["clippy", "rustfmt", "rust-src"] } },
    rustc: { release: "1.99.1", channel: "stable", host: "x86_64-unknown-linux-gnu", commitHash: "58cf8f9c35b355199727627d07ae34a4e2846d67" },
    sysroot: SYSROOT,
    stdlibSources: `${SYSROOT}/lib/rustlib/src/rust/library`,
    components: ["cargo", "clippy", "rust-analyzer", "rust-src", "rust-std", "rustc", "rustfmt"],
    tools: [
      { tool: "rust-analyzer", label: "rust-analyzer", path: `${SYSROOT}/bin/rust-analyzer`, install: "component", licence: "MIT OR Apache-2.0" },
      { tool: "rustfmt", label: "rustfmt", path: `${SYSROOT}/bin/rustfmt`, install: "component", licence: "MIT OR Apache-2.0" },
      { tool: "clippy", label: "Clippy", path: `${SYSROOT}/bin/cargo-clippy`, install: "component", licence: "MIT OR Apache-2.0" },
      { tool: "cargo-nextest", label: "cargo-nextest", path: null, install: "cargo", licence: "MIT OR Apache-2.0" },
      { tool: "cargo-llvm-cov", label: "cargo-llvm-cov", path: "/home/anna/.cargo/bin/cargo-llvm-cov", install: "cargo", licence: "MIT OR Apache-2.0" },
    ],
    prettyPrinters: { etc: `${SYSROOT}/lib/rustlib/etc`, sources: `${SYSROOT}/lib/rustlib/src/rust`, commitHash: "58cf8f9c35b355199727627d07ae34a4e2846d67" },
    chosen,
  };
}

// ---- Cargo workspace (what `cargo metadata` reports for the demo) ------------------------------------

const registry = (name: string, version: string, kind: "normal" | "build" = "normal", children: CargoWorkspace["trees"] = [], features: string[] = []) => ({
  name,
  version,
  source: "registry",
  kind,
  features,
  children,
});
const SYN = registry("syn", "3.0.9", "normal", [registry("proc-macro2", "1.0.107"), registry("quote", "1.0.47"), registry("unicode-ident", "1.0.22")], ["derive", "parsing"]);
const SERDE = registry("serde", "1.0.229", "normal", [registry("serde_core", "1.0.229"), registry("serde_derive", "1.0.229", "normal", [registry("proc-macro2", "1.0.107"), registry("quote", "1.0.47"), SYN])], ["derive", "std"]);
const CORE_TREE = {
  name: "shop-core",
  version: "0.3.0",
  source: "path",
  kind: "normal" as const,
  features: ["default", "json"],
  children: [SERDE, registry("serde_json", "1.0.154", "normal", [registry("itoa", "1.0.18"), registry("memchr", "2.8.0"), { ...registry("serde_core", "1.0.229"), repeated: true }, registry("zmij", "1.0.0")], ["std"])],
};

const WORKSPACE: CargoWorkspace = {
  root: "",
  targetDirectory: "target",
  members: [
    {
      id: "path+file:///home/anna/Projects/shop/crates/shop-core#0.3.0",
      name: "shop-core",
      version: "0.3.0",
      edition: "2021",
      manifest: "crates/shop-core/Cargo.toml",
      dir: "crates/shop-core",
      targets: [
        { name: "shop_core", kind: "lib", srcPath: "crates/shop-core/src/lib.rs", test: true, doctest: true },
        { name: "checkout", kind: "test", srcPath: "crates/shop-core/tests/checkout.rs", test: true, doctest: false },
      ],
      features: [
        { name: "discounts", enables: [], default: false },
        { name: "json", enables: ["dep:serde", "dep:serde_json"], default: true },
      ],
      dependencies: [
        { name: "serde", req: "^1", kind: "normal", optional: true },
        { name: "serde_json", req: "^1", kind: "normal", optional: true },
      ],
    },
    {
      id: "path+file:///home/anna/Projects/shop/crates/shop-cli#0.3.0",
      name: "shop-cli",
      version: "0.3.0",
      edition: "2021",
      manifest: "crates/shop-cli/Cargo.toml",
      dir: "crates/shop-cli",
      targets: [
        { name: "shop", kind: "bin", srcPath: "crates/shop-cli/src/main.rs", test: true, doctest: false },
        { name: "report", kind: "example", srcPath: "crates/shop-cli/examples/report.rs", test: false, doctest: false },
      ],
      features: [],
      dependencies: [
        { name: "anyhow", req: "^1", kind: "normal", optional: false },
        { name: "shop-core", req: "*", kind: "normal", optional: false },
      ],
    },
  ],
  trees: [
    CORE_TREE,
    {
      name: "shop-cli",
      version: "0.3.0",
      source: "path",
      kind: "normal",
      children: [registry("anyhow", "1.0.104", "normal", [], ["default", "std"]), { ...CORE_TREE, children: CORE_TREE.children }],
    },
  ],
};

registerMock("rust_status", () => status());
registerMock("cargo_workspace", () => {
  emitMock("rust://workspace", WORKSPACE);
  return structuredClone(WORKSPACE);
});
registerMock("rust_install_command", ({ kind, name, toolchain }) => {
  const args =
    kind === "toolchain"
      ? ["toolchain", "install", String(name), "--profile", "minimal", "--component", "clippy,rustfmt,rust-src"]
      : kind === "component"
        ? ["component", "add", String(name), ...(toolchain ? ["--toolchain", String(toolchain)] : [])]
        : ["install", "--locked", String(name)];
  return { program: kind === "cargoPlugin" ? "/home/anna/.cargo/bin/cargo" : "/home/anna/.cargo/bin/rustup", args, cwd: "/tmp", env: {} };
});
registerMock("rust_analyzer_download", async () => {
  for (const [i, message] of ["Looking up rust-analyzer 2026-10-19 on GitHub…", "Verified SHA-256 of rust-analyzer-x86_64-unknown-linux-gnu.gz."].entries()) {
    await new Promise((r) => setTimeout(r, 300 + i * 400));
    emitMock("rust://install", { message });
  }
  return "/home/anna/.local/share/forge/langservers/rust-analyzer/2026-10-19/rust-analyzer";
});

const toCommandLine = (c: { program: string; args: string[]; cwd: string; env: Record<string, string> }) => ({
  ...c,
  cwd: c.cwd ? `${RUST_ROOT}/${c.cwd}` : RUST_ROOT,
});
registerMock("cargo_command", async ({ action, package: pkg, target }) =>
  toCommandLine(await call("rust_action_command", { action, package: pkg ?? "", target: target ?? "", context: rustContext() })),
);
registerMock("rust_file_targets", ({ path, content }) => call("rust_run_targets", { path, text: content, context: rustContext() }));
registerMock("rust_format", ({ content }) => content);

// Two of Clippy's findings in the demo (`cargo clippy --message-format=json`), parsed in Rust.
const span = (file: string, line: number, c0: number, c1: number, replacement: string | null = null) =>
  ({ file_name: file, line_start: line, line_end: line, column_start: c0, column_end: c1, is_primary: true, suggested_replacement: replacement, suggestion_applicability: replacement === null ? null : "MachineApplicable" });
const message = (code: string, text: string, file: string, line: number, c0: number, c1: number, help: string, fixes: ReturnType<typeof span>[], rendered: string) =>
  JSON.stringify({
    reason: "compiler-message",
    package_id: "path+file:///home/anna/Projects/shop/crates/shop-core#0.3.0",
    message: {
      message: text,
      code: { code: `clippy::${code}`, explanation: null },
      level: "warning",
      spans: [span(file, line, c0, c1)],
      children: [
        { message: `for further information visit https://rust-lang.github.io/rust-clippy/rust-1.99.0/index.html#${code}`, level: "help", spans: [], children: [] },
        { message: help, level: "help", spans: fixes, children: [] },
      ],
      rendered,
    },
  });
const CLIPPY = [
  message("needless_return", "unneeded `return` statement", "crates/shop-core/src/cart.rs", 50, 9, 37, "remove `return`", [span("crates/shop-core/src/cart.rs", 50, 9, 16, ""), span("crates/shop-core/src/cart.rs", 50, 37, 38, "")], "warning: unneeded `return` statement\n  --> crates/shop-core/src/cart.rs:50:9\n"),
  message("bool_comparison", "equality checks against true are unnecessary", "crates/shop-core/src/money.rs", 14, 8, 27, "try", [span("crates/shop-core/src/money.rs", 14, 8, 27, "cents == 0")], "warning: equality checks against true are unnecessary\n  --> crates/shop-core/src/money.rs:14:8\n"),
  JSON.stringify({ reason: "build-finished", success: true }),
].join("\n");

registerMock("rust_clippy", async ({ path }) => {
  await new Promise((r) => setTimeout(r, 600));
  const all = await call<{ path: string }[]>("rust_problems", { json: CLIPPY, workspace: "" });
  const pkg = typeof path === "string" ? path.match(/^crates\/[^/]+\//)?.[0] : undefined;
  return pkg ? all.filter((p) => p.path.startsWith(pkg)) : all;
});

// Rust files: discovery with the workspace's packages and targets.
const previousDiscover = mockHandler("test_discover");
registerMock("test_discover", async (args) => {
  const path = String(args.path);
  if (path.endsWith(".rs") && isRustDemo()) return call("rust_discover", { path, text: args.text, context: rustContext() });
  return previousDiscover?.(args);
});

// ---- LLDB with rustc's pretty printers --------------------------------------------------------------

const MAIN = "crates/shop-cli/src/main.rs";
const CART = "crates/shop-core/src/cart.rs";

interface RustStep {
  frames: [string, string | null, number][];
  locals: string[];
}

const TRACE: RustStep[] = [
  { frames: [["shop::main", MAIN, 11], ["core::ops::function::FnOnce::call_once<…>", null, 250], ["std::rt::lang_start_internal", null, 175]], locals: ["customer", "cart"] },
  { frames: [["shop::main", MAIN, 12], ["core::ops::function::FnOnce::call_once<…>", null, 250], ["std::rt::lang_start_internal", null, 175]], locals: ["customer", "cart", "quantities"] },
  { frames: [["shop_core::cart::Cart::total", CART, 37], ["shop::main", MAIN, 12], ["core::ops::function::FnOnce::call_once<…>", null, 250]], locals: ["self"] },
  { frames: [["shop::main", MAIN, 13], ["core::ops::function::FnOnce::call_once<…>", null, 250], ["std::rt::lang_start_internal", null, 175]], locals: ["customer", "cart", "quantities", "total"] },
];

interface Value {
  type: string;
  value: string;
  children?: [string, Value][];
}

const str = (s: string): Value => ({ type: "alloc::string::String", value: `"${s}"` });
const item = (name: string, cents: number, quantity: number): Value => ({
  type: "shop_core::cart::Item",
  value: `{name:"${name}", cents:${cents}, quantity:${quantity}}`,
  children: [
    ["name", str(name)],
    ["cents", { type: "i64", value: String(cents) }],
    ["quantity", { type: "u32", value: String(quantity) }],
  ],
});
const ITEMS: Value = { type: "alloc::vec::Vec<shop_core::cart::Item, alloc::alloc::Global>", value: "size=2", children: [["[0]", item("Coffee", 450, 2)], ["[1]", item("Croissant", 220, 3)]] };
const VOUCHER: Value = { type: "core::option::Option<alloc::string::String>", value: 'Some("AUTUMN10")', children: [["0", str("AUTUMN10")]] };
const CART_VALUE: Value = { type: "shop_core::cart::Cart", value: "{items:size=2, voucher:Some(\"AUTUMN10\")}", children: [["items", ITEMS], ["voucher", VOUCHER]] };
const VALUES: Record<string, Value> = {
  customer: str("Anna Schmidt"),
  cart: CART_VALUE,
  self: { type: "&shop_core::cart::Cart", value: "0x00007ffd5c2e1a40", children: [["items", ITEMS], ["voucher", VOUCHER]] },
  quantities: {
    type: "std::collections::hash::map::HashMap<alloc::string::String, u32, std::hash::random::RandomState>",
    value: "size=2",
    children: [
      ['["Coffee"]', { type: "u32", value: "2" }],
      ['["Croissant"]', { type: "u32", value: "3" }],
    ],
  },
  total: { type: "i64", value: "1560" },
};

const ADAPTER: AdapterInfo = { type: "lldb", label: "LLDB", languages: ["rs", "c", "cpp"], launch: true, attachProcess: true, attachPort: false, coreDump: true, command: "/usr/bin/lldb-dap" };

interface RustSession {
  info: SessionInfo;
  pc: number;
  /** Variables handed out, by reference. */
  refs: Map<number, Value>;
}

const sessions = new Map<string, RustSession>();
let seq = 800;
let nextRef = 9100;
const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);
const output = (id: string, stream: string, text: string) => emit({ type: "output", sessionId: id, stream, text });
const top = (s: RustSession) => TRACE[s.pc].frames[0];

function breakpointAt(s: RustSession): Breakpoint | undefined {
  const snap = mockHandler("debug_breakpoints")!({}) as BreakpointsSnapshot;
  if (snap.muted) return undefined;
  const [, file, line] = top(s);
  return snap.breakpoints.find((b) => b.enabled && b.kind === "line" && b.file === file && b.line === line);
}

function stopped(s: RustSession, reason: SessionInfo["reason"]) {
  const [, file, line] = top(s);
  s.info = { ...s.info, status: "paused", reason, threadId: 1, location: { file, fileUri: `${ROOT}/${file}`, line } };
  emit({ type: "session", session: s.info });
}

function finish(s: RustSession) {
  output(s.info.id, "stdout", "Anna Schmidt: 2 items, 15.60 EUR\n");
  output(s.info.id, "console", "Process 51807 exited with status = 0 (0x00000000)\n");
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

function run(s: RustSession, mode: "continue" | "next" | "stepIn" | "stepOut") {
  const depth = (x: number) => (TRACE[x].frames[0][0].startsWith("shop::") ? 1 : 2);
  const start = depth(s.pc);
  for (;;) {
    if (s.pc + 1 >= TRACE.length) return finish(s);
    s.pc += 1;
    if (breakpointAt(s)) return stopped(s, "breakpoint");
    const d = depth(s.pc);
    if (mode === "stepIn" || (mode === "next" && d <= start) || (mode === "stepOut" && d < start)) return stopped(s, "step");
  }
}

function resume(s: RustSession, fn: () => void) {
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => sessions.has(s.info.id) && fn(), 120);
}

function start(request: LaunchRequest): SessionInfo {
  const id = `dap-${++seq}`;
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "rust",
    engine: ADAPTER.label,
    fileUri: `${ROOT}/${MAIN}`,
    file: MAIN,
    peer: "lldb-dap",
    serverName: request.name ?? "shop",
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: "lldb",
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
      restart: true,
      completions: true,
      setVariable: true,
      modules: true,
      loadedSources: false,
      attached: false,
      exceptionFilters: [],
    },
  };
  const s: RustSession = { info, pc: 0, refs: new Map() };
  sessions.set(id, s);
  emit({ type: "session", session: info });
  const init = ((request.configuration as { initCommands?: string[] } | undefined)?.initCommands ?? []).map((c) => `(lldb) ${c}\n`).join("");
  output(id, "console", `${init}(lldb) target create "${request.program ?? `${RUST_ROOT}/target/debug/shop`}"\nLaunching ${request.program}\n`);
  setTimeout(() => {
    if (!sessions.has(id)) return;
    if (breakpointAt(s)) stopped(s, "breakpoint");
    else run(s, "continue");
  }, 500);
  return info;
}

function prop(s: RustSession, name: string, v: Value, fullname = name): Property {
  let reference: number | undefined;
  if (v.children?.length) {
    reference = ++nextRef;
    s.refs.set(reference, v);
  }
  return {
    name,
    fullname,
    type: "dap",
    classname: v.type,
    value: v.value,
    hasChildren: reference !== undefined,
    numChildren: v.children?.length ?? 0,
    page: 0,
    pageSize: 0,
    children: [],
    reference,
  };
}

const own = (args: Record<string, unknown>) => sessions.get(String(args.sessionId ?? ""));

function route(cmd: string, fn: (s: RustSession, args: Record<string, unknown>) => unknown) {
  const previous = mockHandler(cmd);
  registerMock(cmd, (args) => {
    const s = own(args);
    if (s) return fn(s, args);
    return previous ? previous(args) : undefined;
  });
}

if (isRustDemo()) {
  const dapStart = mockHandler("dap_start");
  registerMock("dap_start", (args) => {
    const request = args.request as LaunchRequest;
    return request.adapter === "lldb" ? start(request) : dapStart?.(args);
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
  { id: 1, name: "Thread 1 shop (main)" },
  { id: 2, name: "Thread 2 tokio-runtime-worker" },
]);
route("debug_step", (s, args) => {
  if (s.info.status !== "paused") throw new Error("the program is not stopped");
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
        fileUri: file ? `${ROOT}/${file}` : `${SYSROOT}/lib/rustlib/src/rust/library/core/src/ops/function.rs`,
        line,
        where,
        id: level + 1,
      })),
);
route("debug_contexts", () => [
  { id: 0, name: "Locals" },
  { id: 1, name: "Registers" },
]);
route("debug_variables", (s, args) => {
  if (Number(args.context ?? 0) === 1) return [prop(s, "rip", { type: "unsigned long", value: "0x000055555556a2f4" }), prop(s, "rsp", { type: "unsigned long", value: "0x00007fffffffd9a0" })];
  const level = Number(args.depth ?? 0);
  const names = level === 0 ? TRACE[s.pc].locals : (TRACE.find((t) => t.frames[0][0] === TRACE[s.pc].frames[level]?.[0])?.locals ?? []);
  return names.map((n) => prop(s, n, VALUES[n]));
});
route("debug_property", (s, args) => {
  const ref = Number(args.reference);
  const v = s.refs.get(ref);
  const fullname = String(args.fullname);
  const children = (v?.children ?? []).map(([n, c]) => prop(s, n, c, n.startsWith("[") ? `${fullname}${n}` : `${fullname}.${n}`));
  return { ...prop(s, fullname, v ?? { type: "", value: "" }), reference: ref, hasChildren: children.length > 0, numChildren: children.length, children };
});
route("debug_eval", (s, args) => {
  const e = String(args.expression).trim();
  if (e in VALUES) return prop(s, e, VALUES[e]);
  if (e === "cart.items.len()") return prop(s, e, { type: "usize", value: "2" });
  if (/^-?\d+$/.test(e)) return prop(s, e, { type: "i32", value: e });
  throw new Error(`error: use of undeclared identifier '${e}'`);
});
route("debug_set_value", () => undefined);

registerMock("rust_debug_request", ({ configuration }): LaunchRequest => {
  const cfg = configuration as RunConfiguration;
  const o = cfg.options ?? {};
  const program = cfg.type === "cargo-test" ? `${RUST_ROOT}/target/debug/deps/shop_core-49a7891832f2bb85` : `${RUST_ROOT}/target/debug/${(o.target ?? "bin:shop").startsWith("example:") ? `examples/${(o.target ?? "").slice(8)}` : "shop"}`;
  const etc = `${SYSROOT}/lib/rustlib/etc`;
  return {
    mode: "launch",
    adapter: "lldb",
    name: cfg.name,
    program,
    args: cfg.type === "cargo-test" ? [o.test ?? "", "--exact", "--nocapture", "--test-threads=1"].filter(Boolean) : [],
    cwd: RUST_ROOT,
    configuration: { initCommands: [`command script import "${etc}/lldb_lookup.py"`, `command source -s 1 "${etc}/lldb_commands"`] },
  };
});

// ---- the demo project ----------------------------------------------------------------------------------

if (isRustDemo()) {
  registerMock("project_detect", () => structuredClone(rustDemoInfo));
  setSetting("project.announcedFrameworks", [`${ROOT}#rust`], "user");
  onMockDidOpen((path) => {
    if (!/\.rs$|Cargo\.toml$/.test(path)) return;
    setMockServerStatus({ id: "rust-analyzer", name: "rust-analyzer", state: "ready" });
  });
}

// Screenshot scenarios can start sessions directly.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeRustDebugMock?: unknown }).__forgeRustDebugMock = { start };
}
