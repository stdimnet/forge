/**
 * Browser-mode CForge embedded development for the PlatformIO demo (`?project=pio`):
 * `platformio.ini` and the PlatformIO task commands come from Rust (WebAssembly:
 * `pio_project`, `pio_command`); the outputs of PlatformIO, OpenOCD, idf.py and west are
 * canned. Debugging the OpenOCD Download & Run configuration starts a GDB session that stops
 * in `loop()`, and the Peripherals view decodes the demo SVD file with simulated register
 * values (`svd_summary`, `svd_peripheral_values`). Nothing is flashed or run.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { EmbeddedInfo, EmbeddedLaunch, PioProject } from "@/ipc/cppTools";
import type { DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { LaunchRequest } from "@/features/debug/dapApi";
import type { CommandLine, RunConfiguration, RunRequest } from "@/ipc/run";
import { callWasmSync, forgeWasm } from "@/wasm";
import { cppDemoName, isPioDemo } from "./demoCppProject";
import { registerMockRunHandler, startMockRun, type Step } from "./runMock";
import { mockFiles } from "./workspaceMock";

const ROOT = `/home/anna/Projects/${cppDemoName}`;
const PIO = "/home/anna/.platformio/penv/bin/pio";
const OPENOCD = "/usr/bin/openocd";
const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);

async function project(): Promise<PioProject | null> {
  const text = mockFiles["platformio.ini"];
  if (!text) return null;
  return callWasmSync<PioProject>((await forgeWasm()).pio_project, { text });
}

registerMock("embedded_info", async (): Promise<EmbeddedInfo> => {
  const platformio = await project();
  return { kinds: { platformio: !!platformio, espIdf: false, zephyr: false }, platformio, espIdf: null, zephyr: null };
});

registerMock("pio_command", async ({ env, task }) => callWasmSync<CommandLine>((await forgeWasm()).pio_command, { pio: PIO, env, task, cwd: ROOT }));

registerMock("idf_command", ({ action }) => ({ program: "idf.py", args: [String(action)], cwd: ROOT, env: {} }));
registerMock("west_command", ({ action, board }) => ({
  program: "west",
  args: action === "build" ? ["build", "-b", String(board ?? "nrf52840dk/nrf52840"), "-d", "build", "-p", "auto", "."] : [String(action), "-d", "build"],
  cwd: ROOT,
  env: {},
}));

const ELF = (env: string) => `${ROOT}/.pio/build/${env}/firmware.elf`;

registerMock("cpp_embedded_launch", ({ configuration }): EmbeddedLaunch => {
  const cfg = configuration as RunConfiguration;
  const o = cfg.options ?? {};
  const elf = o.executable ? (o.executable.startsWith("/") ? o.executable : `${ROOT}/${o.executable}`) : ELF("nucleo_f401re");
  const board = o.boardConfig || "board/st_nucleo_f4.cfg";
  const port = Number(o.gdbPort || 3333);
  const gdb = o.gdb || "/usr/bin/gdb-multiarch";
  return {
    server: { program: OPENOCD, args: ["-f", board, "-c", `gdb_port ${port}`], cwd: ROOT, env: {} },
    download: o.download === "never" ? null : { program: OPENOCD, args: ["-f", board, "-c", `program {${elf}} verify reset exit`], cwd: ROOT, env: {} },
    request: { adapter: "gdb", mode: "attachPort", host: "localhost", port, name: cfg.name, program: elf, cwd: ROOT, configuration: { target: `localhost:${port}` }, adapterCommand: gdb } as unknown as LaunchRequest,
    svd: o.svdFile ? (o.svdFile.startsWith("/") ? o.svdFile : `${ROOT}/${o.svdFile}`) : null,
    elf,
  };
});

// ---- Outputs of PlatformIO and OpenOCD ----------------------------------------------------------

const lines = (text: string, ms = 60): Step[] => text.split("\n").map((l): Step => [ms, `${l}\n`]);

function pioBuild(env: string): string {
  const esp = env.startsWith("esp32");
  return `Processing ${env} (platform: ${esp ? "espressif32 @ 6.9.0" : "ststm32"}; board: ${esp ? "esp32dev" : "nucleo_f401re"}; framework: arduino)
--------------------------------------------------------------------------------
Verbose mode can be enabled via \`-v, --verbose\` option
CONFIGURATION: https://docs.platformio.org/page/boards/${esp ? "espressif32/esp32dev" : "ststm32/nucleo_f401re"}.html
PLATFORM: ${esp ? "Espressif 32 (6.9.0) > Espressif ESP32 Dev Module" : "ST STM32 (17.5.0) > ST Nucleo F401RE"}
HARDWARE: ${esp ? "ESP32 240MHz, 320KB RAM, 4MB Flash" : "STM32F401RET6 84MHz, 96KB RAM, 512KB Flash"}
PACKAGES:
 - ${esp ? "framework-arduinoespressif32 @ 3.20017.241212" : "framework-arduinoststm32 @ 4.20900.0 (2.9.0)"}
 - ${esp ? "toolchain-xtensa-esp32 @ 8.4.0+2021r2-patch5" : "toolchain-gccarmnoneeabi @ 1.120301.0 (12.3.1)"}
LDF: Library Dependency Finder -> https://bit.ly/configure-pio-ldf
Dependency Graph
|-- ArduinoJson @ 7.2.1
Compiling .pio/build/${env}/src/blink.cpp.o
Compiling .pio/build/${env}/src/main.cpp.o
Linking .pio/build/${env}/firmware.elf
RAM:   [=         ]   4.6% (used 4504 bytes from 98304 bytes)
Flash: [=         ]   5.4% (used 28312 bytes from 524288 bytes)
Building .pio/build/${env}/firmware.bin
========================= [SUCCESS] Took 3.81 seconds =========================`;
}

const FLASH = `Open On-Chip Debugger 0.12.0
Licensed under GNU GPL v2
Info : The selected transport took over low-level target control.
Info : clock speed 2000 kHz
Info : STLINK V2J39M27 (API v2) VID:PID 0483:374B
Info : Target voltage: 3.252033
Info : [stm32f4x.cpu] Cortex-M4 r0p1 processor detected
Info : starting gdb server for stm32f4x.cpu on 3333
** Programming Started **
Info : device id = 0x10006433
Info : flash size = 512 KiB
** Programming Finished **
** Verify Started **
** Verified OK **
** Resetting Target **
shutdown command invoked`;

const SERVER = `Open On-Chip Debugger 0.12.0
Licensed under GNU GPL v2
Info : Listening on port 6666 for tcl connections
Info : Listening on port 4444 for telnet connections
Info : clock speed 2000 kHz
Info : STLINK V2J39M27 (API v2) VID:PID 0483:374B
Info : Target voltage: 3.252033
Info : [stm32f4x.cpu] Cortex-M4 r0p1 processor detected
Info : [stm32f4x.cpu] target has 6 breakpoints, 4 watchpoints
Info : starting gdb server for stm32f4x.cpu on 3333
Info : Listening on port 3333 for gdb connections
Info : accepting 'gdb' connection on tcp/3333
Info : device id = 0x10006433`;

function stepsFor(cmd: CommandLine): Step[] | null {
  const program = cmd.program.split("/").pop() ?? "";
  const a = cmd.args;
  const env = a[a.indexOf("-e") + 1] && a.includes("-e") ? a[a.indexOf("-e") + 1] : "nucleo_f401re";
  if (program === "pio") {
    const target = a.includes("-t") ? a[a.indexOf("-t") + 1] : "";
    if (a[0] === "device") return lines(`--- Terminal on /dev/ttyACM0 | 115200 8-N-1\n--- Quit: Ctrl+C | Menu: Ctrl+T | Help: Ctrl+T followed by Ctrl+H\nblinky ready\nled on after 250 ms\nled off after 500 ms\nled on after 750 ms`, 250);
    if (a[0] === "test")
      return [
        ...lines(`Verbose mode can be enabled via \`-v, --verbose\` option\nCollected 1 tests\n\nProcessing test_blink in native environment\n--------------------------------------------------------------------------------\nBuilding...\nTesting...\ntest/test_blink/test_main.cpp:24: test_toggles_after_the_period\t[PASSED]\ntest/test_blink/test_main.cpp:25: test_waits_for_the_next_period\t[PASSED]\n----------------- native:test_blink [PASSED] Took 1.52 seconds -----------------\n\n==================================== SUMMARY ====================================\nEnvironment    Test        Status    Duration\n-------------  ----------  --------  ------------\nnative         test_blink  PASSED    00:00:01.523\n=================== 2 test cases: 2 succeeded in 00:00:01.523 ===================`, 40),
        [40, { code: 0 }],
      ];
    if (target === "clean") return [...lines(`Removing .pio/build/${env}\nDone cleaning`), [40, { code: 0 }]];
    if (target === "compiledb") return [...lines(`Processing ${env} (platform: ststm32; board: nucleo_f401re; framework: arduino)\nBuilding compilation database compile_commands.json\n========================= [SUCCESS] Took 1.12 seconds =========================`), [40, { code: 0 }]];
    const upload = target === "upload" ? `\nConfiguring upload protocol...\nAVAILABLE: blackmagic, cmsis-dap, jlink, stlink\nCURRENT: upload_protocol = stlink\nUploading .pio/build/${env}/firmware.elf\n${FLASH}\n========================= [SUCCESS] Took 6.20 seconds =========================` : "";
    return [...lines(pioBuild(a.includes("-e") ? env : "nucleo_f401re") + upload, 45), [40, { code: 0 }]];
  }
  if (program === "openocd") {
    if (a.some((x) => x.startsWith("program "))) return [...lines(FLASH, 70), [40, { code: 0 }]];
    return lines(SERVER, 50);
  }
  if (program === "idf.py" || program === "west") return [...lines(`Running ${program} ${a.join(" ")} (not available in browser mode)`), [40, { code: 1 }]];
  return null;
}

registerMockRunHandler({
  commandLine: async (r: RunRequest) => (r.command && stepsFor(r.command) ? r.command : null),
  start: async (r: RunRequest, runId: string) => {
    if (!isPioDemo() && r.command?.program !== OPENOCD) return null;
    const steps = r.command ? stepsFor(r.command) : null;
    return steps ? startMockRun(runId, r.command!, steps) : null;
  },
});

// ---- The GDB session and the Peripherals view ---------------------------------------------------

const FILE = "src/main.cpp";
interface Session {
  info: SessionInfo;
  line: number;
}
const sessions = new Map<string, Session>();
let seq = 0;
const isEmbedded = (args: Record<string, unknown>) => sessions.has(String(args.sessionId ?? ""));

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
  container: 2000,
});

function start(request: LaunchRequest): SessionInfo {
  const id = `dap-embedded-${++seq}`;
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "cpp",
    engine: "GDB",
    fileUri: `${ROOT}/${FILE}`,
    file: FILE,
    peer: "gdb-multiarch -i dap (target remote localhost:3333)",
    serverName: request.name ?? "blinky",
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: "gdb",
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
      completions: false,
      setVariable: true,
      modules: false,
      loadedSources: false,
      attached: true,
      exceptionFilters: [],
    },
  };
  const s: Session = { info, line: 15 };
  sessions.set(id, s);
  emit({ type: "session", session: info });
  emit({ type: "output", sessionId: id, stream: "system", text: `Remote debugging using localhost:3333\nloop () at ${ROOT}/${FILE}:15\n` });
  setTimeout(() => {
    if (!sessions.has(id)) return;
    s.info = { ...s.info, status: "paused", reason: "breakpoint", location: { file: FILE, fileUri: `${ROOT}/${FILE}`, line: s.line } };
    emit({ type: "session", session: s.info });
  }, 300);
  return info;
}

function end(s: Session): void {
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

const previousStart = mockHandler("dap_start")!;
registerMock("dap_start", (args) => {
  const request = args.request as LaunchRequest & { mode?: string };
  return request.mode === "attachPort" && (request.program ?? "").endsWith(".elf") ? start(request) : previousStart(args);
});

function route(cmd: string, fn: (s: Session, args: Record<string, unknown>) => unknown) {
  const previous = mockHandler(cmd)!;
  registerMock(cmd, (args) => (isEmbedded(args) ? fn(sessions.get(String(args.sessionId))!, args) : previous(args)));
}

for (const cmd of ["debug_status", "debug_sessions"]) {
  const previous = mockHandler(cmd)!;
  registerMock(cmd, async (args) => {
    const base = (await previous(args)) as SessionInfo[] | { sessions: SessionInfo[] };
    const mine = [...sessions.values()].map((s) => s.info);
    return Array.isArray(base) ? [...base, ...mine] : { ...base, sessions: [...base.sessions, ...mine] };
  });
}

const frames = (s: Session): Frame[] => [
  { level: 0, file: FILE, fileUri: `${ROOT}/${FILE}`, line: s.line, where: "loop()", id: 1, address: "0x08000412" },
  { level: 1, file: null, fileUri: "", line: 0, where: "main", id: 2, address: "0x08001a6c" },
  { level: 2, file: null, fileUri: "", line: 0, where: "Reset_Handler", id: 3, address: "0x08000c2e" },
];

route("debug_step", (s, args) => {
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => {
    if (!sessions.has(s.info.id)) return;
    if (args.kind !== "run") s.line = s.line >= 16 ? 14 : s.line + 1;
    s.info = { ...s.info, status: "paused", reason: args.kind === "run" ? "breakpoint" : "step", location: { file: FILE, fileUri: `${ROOT}/${FILE}`, line: s.line } };
    emit({ type: "session", session: s.info });
  }, 150);
});
route("dap_threads", () => [{ id: 1, name: "stm32f4x.cpu" }]);
route("debug_pause", () => undefined);
route("debug_stop", (s) => end(s));
route("debug_detach", (s) => end(s));
route("debug_stack", (s) => frames(s));
route("debug_contexts", () => [
  { id: 0, name: "Locals" },
  { id: 1, name: "Registers" },
]);
route("debug_variables", (_s, args) =>
  Number(args.context ?? 0) === 1
    ? [prop("pc", "0x08000412 <loop()+18>", "uint32_t"), prop("sp", "0x2001ffe8", "uint32_t"), prop("lr", "0x08001a6d", "uint32_t"), prop("xpsr", "0x61000000", "uint32_t")]
    : [prop("blinker", "{pin_ = 5 '\\005', period_ = 250, last_ = 1250, on_ = true}", "Blinker", 2100, 4)],
);
route("debug_property", (_s, args) => {
  const list =
    Number(args.reference) === 2100
      ? [prop("pin_", "5 '\\005'", "uint8_t"), prop("period_", "250", "uint32_t"), prop("last_", "1250", "uint32_t"), prop("on_", "true", "bool")]
      : [];
  return { name: String(args.fullname), fullname: String(args.fullname), type: "dap", hasChildren: true, numChildren: list.length, page: 0, pageSize: 0, children: list, reference: Number(args.reference) };
});

const svdText = (path: string) => mockFiles[path.startsWith(`${ROOT}/`) ? path.slice(ROOT.length + 1) : path] ?? "";

registerMock("cpp_svd_peripherals", async ({ path }) => {
  const text = svdText(String(path));
  if (!text) throw new Error(`Cannot read ${path}`);
  return callWasmSync((await forgeWasm()).svd_summary, { text });
});

/** Register values of the simulated board: PA5 is an output and on, the PLL runs. */
const VALUES: Record<string, Record<string, number>> = {
  GPIOA: { MODER: 0xa8000400, IDR: 0x0000a020, ODR: 0x00000020 },
  GPIOB: { MODER: 0x00000280, IDR: 0x00000010 },
  RCC: { CR: 0x03036783, AHB1ENR: 0x00100001 },
  USART2: { SR: 0x00c0, DR: 0x000a },
  TIM2: { CCR1: 250, CCR2: 0, "DMA.DCR": 0x0305 },
};

registerMock("cpp_svd_read", async ({ path, peripheral }) => {
  const text = svdText(String(path));
  return callWasmSync((await forgeWasm()).svd_peripheral_values, { text, peripheral, values: VALUES[String(peripheral)] ?? {} });
});

// The demo's run configuration, like a CLion user's embedded project.
if (isPioDemo()) {
  void Promise.resolve().then(async () => {
    const save = mockHandler("run_configurations_save");
    if (!save) return;
    await save({
      configurations: {
        selected: "blinky (OpenOCD)",
        configurations: [
          {
            name: "blinky (OpenOCD)",
            type: "openocd",
            storeAsProjectFile: true,
            options: { executable: ".pio/build/nucleo_f401re/firmware.elf", boardConfig: "board/st_nucleo_f4.cfg", gdbPort: "3333", download: "always", svdFile: "STM32F401.svd" },
          },
        ],
      },
    });
  });
}
