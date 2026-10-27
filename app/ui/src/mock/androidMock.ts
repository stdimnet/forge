/**
 * Browser-mode Android (JavaForge): the `android_*` commands and Android App runs on the
 * demo app (`?project=android`, demoAndroidProject.ts). adb's and the emulator's output is
 * recorded; parsing it, the Android modules and manifests, the run's command line and
 * Logcat's entries, packages, stack trace links and query filtering go through the Rust
 * `javaforge-android` crate (WebAssembly), like in the desktop app. `?android-sdk=missing`
 * starts without an SDK so the hint can be seen. No device is touched.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { AndroidModuleInfo, AndroidStatus, Device, DevicesSnapshot, LogcatBatch, LogcatView, LogEntry } from "@/ipc/android";
import type { CommandLine, RunConfiguration, RunRequest } from "@/ipc/run";
import { callWasmSync, forgeWasm } from "@/wasm";
import { adbDevicesOutput, androidDemoInfo, avdsOutput, isAndroidDemo, logcatOutput, psOutput } from "./demoAndroidProject";
import { mockFiles } from "./workspaceMock";
import { mockSettingValue } from "./settingsMock";
import { projectRoot, startMockRun, type Step } from "./runMock";

const HOME = "/home/anna";
const ROOT = () => projectRoot().replace(/^~/, HOME);
const SDK = `${HOME}/Android/Sdk`;
const noSdk = () => typeof location !== "undefined" && new URLSearchParams(location.search).get("android-sdk") === "missing";
const NO_SDK_HINT =
  "The Android SDK was not found. Install it with Android Studio or the command-line tools (developer.android.com/studio), then set ANDROID_HOME, sdk.dir in local.properties, or Settings | Languages & Frameworks | Android SDK. Forge does not download it.";

if (isAndroidDemo()) {
  registerMock("project_detect", () => structuredClone(androidDemoInfo));
}

async function modules(): Promise<AndroidModuleInfo[]> {
  const wasm = await forgeWasm();
  const out: AndroidModuleInfo[] = [];
  for (const dir of ["app"]) {
    const build = mockFiles[`${dir}/build.gradle.kts`] ?? mockFiles[`${dir}/build.gradle`];
    if (!build) continue;
    const r = callWasmSync<{ module: Omit<AndroidModuleInfo, "dir" | "gradlePath"> | null }>(wasm.android_module, { build });
    if (r.module) out.push({ dir, gradlePath: `:${dir}`, ...r.module });
  }
  return out;
}

registerMock("android_status", async (): Promise<AndroidStatus> => {
  const sdk = noSdk() ? null : { path: String(mockSettingValue("android.sdk.path") ?? "").trim() || SDK, source: mockSettingValue("android.sdk.path") ? "setting" : "ANDROID_HOME" };
  return {
    sdk,
    adb: sdk ? `${sdk.path}/platform-tools/adb` : null,
    emulator: sdk ? `${sdk.path}/emulator/emulator` : null,
    hint: sdk ? null : NO_SDK_HINT,
    modules: await modules(),
    remote: false,
  };
});

/** AVDs started in this session (`emulator-5556` …). */
const started = new Map<string, string>();

registerMock("android_devices", async (): Promise<DevicesSnapshot> => {
  if (noSdk()) return { devices: [], avds: [], adb: null, emulator: null, hint: NO_SDK_HINT, error: null };
  const wasm = await forgeWasm();
  const parsed = callWasmSync<{ devices: Device[]; avds: string[] }>(wasm.android_adb, { devices: adbDevicesOutput, avds: avdsOutput });
  const devices = parsed.devices.map((d) => (d.serial === "emulator-5554" ? { ...d, avd: "Pixel_8_API_35" } : d));
  for (const [avd, serial] of started) {
    devices.push({ serial, state: "device", model: "sdk_gphone64_x86_64", product: "sdk_gphone64_x86_64", device: "emu64xa", transportId: "7", usb: null, emulator: true, avd });
  }
  return {
    devices,
    avds: parsed.avds.map((name) => ({ name, serial: devices.find((d) => d.avd === name)?.serial ?? null })),
    adb: `${SDK}/platform-tools/adb`,
    emulator: `${SDK}/emulator/emulator`,
    hint: null,
    error: null,
  };
});

registerMock("android_emulator_start", ({ avd }) => {
  const name = String(avd);
  setTimeout(() => started.set(name, `emulator-${5556 + started.size * 2}`), 1500);
});

// ---- Logcat ---------------------------------------------------------------------------------

let serial: string | null = null;
let query = "";
let text = "";
let timer: ReturnType<typeof setInterval> | undefined;
let lastId = 0;

const sources = () => Object.keys(mockFiles).filter((f) => f.endsWith(".kt") || f.endsWith(".java"));

async function view(limit = 5000): Promise<LogcatView> {
  const wasm = await forgeWasm();
  return callWasmSync<LogcatView>(wasm.android_logcat_view, { text, query, mine: ["com.acme.notes"], sources: sources(), ps: psOutput, limit });
}

/** Lines the demo device logs while Logcat is open. */
const LIVE = [
  "D OkHttp  : --> GET https://api.acme.com/notes?since=1730024050",
  "D OkHttp  : <-- 200 OK https://api.acme.com/notes (184ms, 2.1 kB)",
  "I NoteRepository: Synced 3 notes",
  "W Choreographer: Skipped 12 frames!  The application may be doing too much work on its main thread.",
];

function stamp(i: number) {
  const s = 20 + i;
  return `10-27 10:14:${String(s % 60).padStart(2, "0")}.${String((i * 137) % 1000).padStart(3, "0")}`;
}

registerMock("android_logcat_start", async (args) => {
  if (timer) clearInterval(timer);
  serial = String(args.serial);
  query = String(args.query ?? mockSettingValue("android.logcat.defaultQuery") ?? "package:mine");
  text = logcatOutput;
  lastId = (await view(100000)).total;
  let i = 0;
  timer = setInterval(async () => {
    const line = LIVE[i % LIVE.length]!;
    const pid = line.includes("Choreographer") ? "2210  2210" : "12034 12060";
    text += `${stamp(i)} ${pid} ${line}\n`;
    i++;
    const v = await view(100000);
    const fresh: LogEntry[] = v.entries.filter((e) => e.id > lastId);
    lastId = v.total;
    if (fresh.length) emitMock("android://logcat", { serial: serial!, entries: fresh } satisfies LogcatBatch);
    if (i >= 12 && timer) clearInterval(timer);
  }, 2500);
});

registerMock("android_logcat_view", async (args) => {
  query = String(args.query ?? "");
  return view(Number(args.limit ?? 5000));
});

registerMock("android_logcat_stop", () => {
  if (timer) clearInterval(timer);
});

registerMock("android_logcat_clear", () => {
  text = "";
  lastId = 0;
  if (serial) emitMock("android://logcat", { serial, entries: [], cleared: true } satisfies LogcatBatch);
});

// ---- Android App runs --------------------------------------------------------------------------

async function appCommand(cfg: RunConfiguration): Promise<CommandLine> {
  const wasm = await forgeWasm();
  const build = mockFiles["app/build.gradle.kts"] ?? "";
  const info = callWasmSync<{ module: { namespace?: string; applicationId?: string } | null; launcher: string | null }>(wasm.android_module, {
    build,
    manifest: mockFiles["app/src/main/AndroidManifest.xml"] ?? null,
  });
  const out = callWasmSync<{ program: string; args: string[] }>(wasm.android_app_command, {
    run: {
      gradle: `${ROOT()}/gradlew`,
      gradleArgs: [],
      modulePath: `:${cfg.options?.module || "app"}`,
      moduleDir: cfg.options?.module || "app",
      variant: cfg.options?.variant || "debug",
      apk: cfg.options?.apk ?? "",
      adb: `${SDK}/platform-tools/adb`,
      serial: cfg.options?.device || "emulator-5554",
      package: info.module?.applicationId ?? info.module?.namespace ?? "",
      activity: cfg.options?.activity || info.launcher || "",
      skipBuild: false,
    },
  });
  return { program: out.program, args: out.args, cwd: ROOT(), env: { ANDROID_HOME: SDK } };
}

function appSteps(cfg: RunConfiguration): Step[] {
  const device = cfg.options?.device || "emulator-5554";
  return [
    [100, "> gradlew --console=plain :app:assembleDebug\n"],
    [600, "> Task :app:preBuild UP-TO-DATE\n"],
    [200, "> Task :app:compileDebugKotlin\n"],
    [300, "> Task :app:dexBuilderDebug\n"],
    [250, "> Task :app:packageDebug\n"],
    [150, "> Task :app:assembleDebug\n\nBUILD SUCCESSFUL in 7s\n34 actionable tasks: 6 executed, 28 up-to-date\n"],
    [100, `> adb -s ${device} install -r -t app/build/outputs/apk/debug/app-debug.apk\n`],
    [700, "Performing Streamed Install\nSuccess\n"],
    [100, `> adb -s ${device} shell am start -n com.acme.notes/com.acme.notes.MainActivity -a android.intent.action.MAIN -c android.intent.category.LAUNCHER\n`],
    [300, "Starting: Intent { act=android.intent.action.MAIN cat=[android.intent.category.LAUNCHER] cmp=com.acme.notes/.MainActivity }\n"],
    [60, { code: 0 }],
  ];
}

const prevStart = mockHandler("run_start")!;
registerMock("run_start", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || cfg.type !== "android-app") return prevStart(args);
  const line = await appCommand(cfg);
  return startMockRun(r.runId ?? `mock-android-${Date.now()}`, line, appSteps(cfg), { outputFilters: r.outputFilters });
});

const prevCommandLine = mockHandler("run_command_line")!;
registerMock("run_command_line", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || cfg.type !== "android-app") return prevCommandLine(args);
  return appCommand(cfg);
});

if (isAndroidDemo()) {
  void Promise.resolve().then(async () => {
    const save = mockHandler("run_configurations_save");
    if (!save) return;
    await save({
      configurations: {
        selected: "app",
        configurations: [
          { name: "app", type: "android-app", storeAsProjectFile: true, options: { module: "app", variant: "debug" } },
          { name: "release.groovy", type: "groovy-script", options: { script: "scripts/release.groovy" }, arguments: "1.5.0" },
          { name: "notes-app [test]", type: "gradle", storeAsProjectFile: true, options: { tasks: ":app:testDebugUnitTest" } },
        ],
      },
    });
  });
}
