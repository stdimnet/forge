/**
 * CForge's embedded development in the UI (CLion's PlatformIO, ESP-IDF, Zephyr and
 * OpenOCD support). Rust detects the frameworks and builds every command line
 * (`embedded_info`, `pio_command`, `idf_command`, `west_command`, `cpp_embedded_launch`):
 *
 * - the PlatformIO tool window (environments and their tasks; Compilation Database
 *   restarts clangd on the generated `compile_commands.json`),
 * - Tools | ESP-IDF (build, flash, monitor; menuconfig in the terminal) and Tools | Zephyr
 *   (west build / flash with the chosen board; west debug in the terminal),
 * - the "OpenOCD Download & Run" and "Embedded GDB Server" run configurations: flash with
 *   OpenOCD, start the GDB server, debug with GDB (`target remote`) through the shared DAP
 *   layer, and the Peripherals tab of the Debug tool window for the configuration's SVD file,
 * - Settings | Build, Execution, Deployment | Embedded Development.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { CircuitBoard, Cpu, Microchip } from "lucide-solid";
import { cppToolsApi, type EmbeddedInfo, type IdfAction, type PioTask, type WestAction } from "@/ipc/cppTools";
import { runApi, type CommandLine, type RunConfiguration } from "@/ipc/run";
import { registerKeymapActions } from "@/keymap";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { action, group, registerMainMenuEntries } from "@/features/mainMenu/model";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { dapApi } from "@/features/debug/dapApi";
import { registerDebugTab } from "@/features/debug/debugTabs";
import { sessions as debugSessions } from "@/features/debug/store";
import { setInputDialog } from "@/features/git/overlays";
import { openTerminalWith } from "@/features/terminal";
import { safeMode } from "@/features/trust/store";
import { baseName, registerDebugLauncher, registerRunConfigurationType } from "@/features/run/types";
import { stopSession, sessions as runSessions } from "@/features/run/runStore";
import { isDialogWindow } from "@/shell/window/mode";
import { registerCommand } from "@/state/commands";
import { showToolWindow, toggleToolWindow } from "@/state/layout";
import { saveSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { PeripheralsView, setSessionSvd, svdOf } from "./PeripheralsView";
import { PlatformIOToolWindow } from "./PlatformIOToolWindow";
import { buildFirst, cppActive, executableTargets, runTool, showCppBalloon } from "./state";
import "./embedded.css";
import { t } from "@/i18n";

const [info, setInfo] = createSignal<EmbeddedInfo | null>(null);
export { info as embeddedInfo };

export async function refreshEmbedded(): Promise<EmbeddedInfo | null> {
  const i = await cppToolsApi.embeddedInfo().catch(() => null);
  setInfo(i);
  return i;
}

const errorText = (e: unknown) => String(e).replace(/^Error: /, "");
const pioActive = () => cppActive() && !!info()?.kinds.platformio;
const idfActive = () => cppActive() && !!info()?.kinds.espIdf;
const zephyrActive = () => cppActive() && !!info()?.kinds.zephyr;

// ---- PlatformIO -----------------------------------------------------------------------------

const TASK_LABELS: Record<PioTask, string> = {
  build: "Build",
  upload: "Upload",
  monitor: "Monitor",
  test: "Test",
  clean: "Clean",
  compiledb: "Compilation Database",
};

/** Run a PlatformIO task of an environment (all default environments when `env` is empty). */
export async function runPioTask(env: string, task: PioTask): Promise<void> {
  try {
    const cmd = await cppToolsApi.pioCommand(env, task);
    const handle = await runTool(`PlatformIO: ${TASK_LABELS[task]}${env ? ` [${env}]` : ""}`, cmd);
    if (task === "compiledb" && handle && (await handle.exited) === 0) {
      await cppToolsApi.restartClangd().catch(() => undefined);
      showCppBalloon("Compilation database generated", "clangd reads compile_commands.json of PlatformIO now.", 4000);
    }
  } catch (e) {
    showCppBalloon("Cannot run PlatformIO", errorText(e), 8000);
  }
}

registerToolWindow({
  id: "platformio",
  title: t("toolwindow.platformio.title"),
  icon: CircuitBoard,
  anchor: "right",
  order: 30,
  component: PlatformIOToolWindow,
  when: pioActive,
});

// ---- ESP-IDF and Zephyr ---------------------------------------------------------------------

const IDF_LABELS: Record<IdfAction, string> = {
  build: "Build",
  flash: "Flash",
  monitor: "Monitor",
  menuconfig: "Menuconfig",
  fullClean: "Full Clean",
};

/** Interactive commands (menuconfig, west debug) need a terminal. */
function inTerminal(title: string, cmd: CommandLine) {
  if (safeMode()) {
    showCppBalloon(`Cannot run ${title}`, "The project is in Safe Mode: trust it to run its build tools.", 6000);
    return;
  }
  openTerminalWith({ title, program: cmd.program, args: cmd.args, env: Object.entries(cmd.env ?? {}) });
}

async function idf(action: IdfAction): Promise<void> {
  try {
    const cmd = await cppToolsApi.idfCommand(action);
    if (action === "menuconfig") inTerminal("ESP-IDF: Menuconfig", cmd);
    else await runTool(`ESP-IDF: ${IDF_LABELS[action]}`, cmd);
  } catch (e) {
    showCppBalloon("Cannot run idf.py", errorText(e), 8000);
  }
}

function chooseBoard(then?: (board: string) => void): void {
  const z = info()?.zephyr;
  const known = z?.localBoards ?? [];
  setInputDialog({
    title: "Zephyr Board",
    label: `Board to build for${known.length ? ` (this application: ${known.join(", ")})` : ""}:`,
    value: z?.board ?? known[0] ?? "nrf52840dk/nrf52840",
    okLabel: "Select",
    onOk: (value) => {
      const board = value.trim();
      if (!board) return;
      void saveSetting("zephyr.board", board).then(() => refreshEmbedded());
      then?.(board);
    },
  });
}

async function west(action: WestAction, board?: string): Promise<void> {
  const chosen = board ?? info()?.zephyr?.board ?? undefined;
  if ((action === "build" || action === "rebuild") && !chosen) return chooseBoard((b) => void west(action, b));
  try {
    const cmd = await cppToolsApi.westCommand(action, chosen);
    if (action === "debug") inTerminal("Zephyr: west debug", cmd);
    else await runTool(`west ${action === "rebuild" ? "build (pristine)" : action}${chosen && action !== "flash" ? ` [${chosen}]` : ""}`, cmd);
  } catch (e) {
    showCppBalloon("Cannot run west", errorText(e), 8000);
  }
}

// ---- OpenOCD Download & Run / Embedded GDB Server --------------------------------------------

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const targetField = {
  key: "target",
  label: "Target",
  kind: "enum" as const,
  options: () => [{ value: "", label: "(use the executable)" }, ...executableTargets().map((t) => ({ value: t.name, label: t.name }))],
};
const elfField = { key: "executable", label: "Executable (ELF)", kind: "path" as const, placeholder: "Empty: the target's executable" };
const svdField = { key: "svdFile", label: "SVD file (Peripherals view)", kind: "path" as const, placeholder: "Empty: Settings | Embedded Development" };
const gdbField = { key: "gdb", label: "GDB", kind: "path" as const, placeholder: "Empty: the cross toolchain's GDB or gdb-multiarch" };

registerRunConfigurationType({
  id: "openocd",
  name: "OpenOCD Download & Run",
  description: "Flash the target's ELF with OpenOCD, run it on the board or debug it with GDB through OpenOCD's GDB server.",
  icon: Cpu,
  color: "var(--syn-keyword)",
  pluginId: "forge.cpp",
  argumentsLabel: false,
  debuggable: true,
  noRunTarget: true,
  defaults: () => ({ options: { target: executableTargets()[0]?.name ?? "", gdbPort: "3333", download: "always" } }),
  fields: [
    targetField,
    elfField,
    { key: "boardConfig", label: "Board config file", kind: "string", placeholder: "board/st_nucleo_f4.cfg" },
    { key: "gdbPort", label: "GDB port", kind: "string", default: "3333" },
    {
      key: "download",
      label: "Download",
      kind: "segmented",
      default: "always",
      options: [
        { value: "always", label: "Always" },
        { value: "never", label: "Never" },
      ],
    },
    svdField,
    gdbField,
    { key: "openocdArguments", label: "OpenOCD arguments", kind: "string" },
  ],
  suggestName: (cfg) => opt(cfg, "target") || baseName(opt(cfg, "executable")),
  // Run: flash and reset the board.
  start: async (cfg, request) => {
    if (opt(cfg, "target") && !(await buildFirst({ ...cfg, type: "cmake-app" }))) throw new Error("The build failed: see the Build problems");
    const l = await cppToolsApi.embeddedLaunch(cfg);
    if (!l.download) throw new Error("Download is set to Never: nothing to run. Debug the configuration instead.");
    return runApi.start({ ...request, configuration: undefined, command: l.download });
  },
});

registerRunConfigurationType({
  id: "embedded-gdb-server",
  name: "Embedded GDB Server",
  description: "Start a GDB server (J-Link, ST-LINK, pyOCD, QEMU) and debug the target's ELF with GDB connected to it.",
  icon: Microchip,
  color: "var(--syn-keyword)",
  pluginId: "forge.cpp",
  argumentsLabel: false,
  debuggable: true,
  noRunTarget: true,
  defaults: () => ({ options: { target: executableTargets()[0]?.name ?? "", targetRemote: "localhost:3333" } }),
  fields: [
    targetField,
    elfField,
    { key: "targetRemote", label: "'target remote' args", kind: "string", default: "localhost:3333" },
    { key: "gdbServer", label: "GDB server", kind: "path", placeholder: "JLinkGDBServer, st-util, pyocd (empty: already running)" },
    { key: "gdbServerArguments", label: "GDB server args", kind: "string", placeholder: "-device STM32F401RE -if SWD -port 3333" },
    svdField,
    gdbField,
  ],
  suggestName: (cfg) => opt(cfg, "target") || baseName(opt(cfg, "executable")),
  validate: () => null,
});

/** Wait until a run prints the GDB server's ready line (or a moment passed). */
async function serverReady(runId: string): Promise<void> {
  await new Promise<void>((resolve) => {
    let off: (() => void) | undefined;
    const done = () => {
      off?.();
      clearTimeout(timer);
      resolve();
    };
    const timer = setTimeout(done, 3000);
    void runApi
      .onEvent(runId, (e) => {
        if ((e.type === "output" && /for gdb connections|Listening on port \d+/i.test(e.text)) || e.type === "exited") done();
      })
      .then((u) => (off = u));
  });
}

const EMBEDDED_TYPES = new Set(["openocd", "embedded-gdb-server"]);

registerDebugLauncher({
  id: "cforge-embedded",
  canDebug: (cfg) => EMBEDDED_TYPES.has(cfg.type),
  async launch(cfg) {
    try {
      if (opt(cfg, "target") && !(await buildFirst({ ...cfg, type: "cmake-app" }))) return;
      const l = await cppToolsApi.embeddedLaunch(cfg);
      if (l.download) {
        const flashed = await runTool(`Download ${cfg.name}`, l.download);
        if (!flashed || (await flashed.exited) !== 0) {
          showCppBalloon(`Cannot debug '${cfg.name}'`, "Flashing the board failed: see the Run tool window.", 6000);
          return;
        }
      }
      let serverTab: string | undefined;
      if (l.server) {
        const server = await runTool(`${cfg.type === "openocd" ? "OpenOCD" : "GDB Server"}: ${cfg.name}`, l.server);
        if (!server) return;
        serverTab = runSessions().find((s) => s.runId === server.started.runId)?.id;
        await serverReady(server.started.runId);
      }
      await refreshDebugAdapters();
      showToolWindow("debug");
      const session = await dapApi.start(l.request);
      if (l.svd) setSessionSvd(session.id, l.svd);
      // The GDB server stops with the debug session.
      if (serverTab) {
        createRoot((dispose) =>
          createEffect(
            on(debugSessions, (list) => {
              if (list.some((s) => s.id === session.id)) return;
              dispose();
              void stopSession(serverTab);
            }, { defer: true }),
          ),
        );
      }
    } catch (e) {
      showCppBalloon(`Cannot debug '${cfg.name}'`, errorText(e), 8000);
    }
  },
});

registerDebugTab({
  id: "peripherals",
  title: "Peripherals",
  when: (s) => !!s && !!svdOf(s.id),
  component: PeripheralsView,
});

// ---- Actions, menus and settings ------------------------------------------------------------

registerKeymapActions([
  { id: "ActivatePlatformIOToolWindow", title: "PlatformIO", group: "Tool Windows" },
  { id: "cpp.espidf.build", title: "ESP-IDF: Build", group: "Plugins/C/C++" },
  { id: "cpp.espidf.flash", title: "ESP-IDF: Flash", group: "Plugins/C/C++" },
  { id: "cpp.espidf.monitor", title: "ESP-IDF: Monitor", group: "Plugins/C/C++" },
  { id: "cpp.espidf.menuconfig", title: "ESP-IDF: Menuconfig", group: "Plugins/C/C++" },
  { id: "cpp.zephyr.build", title: "Zephyr: West Build", group: "Plugins/C/C++" },
  { id: "cpp.zephyr.flash", title: "Zephyr: West Flash", group: "Plugins/C/C++" },
  { id: "cpp.zephyr.debug", title: "Zephyr: West Debug", group: "Plugins/C/C++" },
  { id: "cpp.zephyr.board", title: "Zephyr: Select Board…", group: "Plugins/C/C++" },
  { id: "cpp.platformio.refresh", title: "Refresh PlatformIO Project", group: "Plugins/C/C++" },
]);

registerCommand({ id: "ActivatePlatformIOToolWindow", title: "PlatformIO", category: "View", run: () => toggleToolWindow("platformio") });
registerCommand({ id: "cpp.platformio.refresh", title: "Refresh PlatformIO Project", category: "PlatformIO", run: () => void refreshEmbedded() });
registerCommand({ id: "cpp.espidf.build", title: "ESP-IDF: Build", category: "ESP-IDF", run: () => void idf("build") });
registerCommand({ id: "cpp.espidf.flash", title: "ESP-IDF: Flash", category: "ESP-IDF", run: () => void idf("flash") });
registerCommand({ id: "cpp.espidf.monitor", title: "ESP-IDF: Monitor", category: "ESP-IDF", run: () => void idf("monitor") });
registerCommand({ id: "cpp.espidf.menuconfig", title: "ESP-IDF: Menuconfig", category: "ESP-IDF", run: () => void idf("menuconfig") });
registerCommand({ id: "cpp.zephyr.build", title: "Zephyr: West Build", category: "Zephyr", run: () => void west("build") });
registerCommand({ id: "cpp.zephyr.flash", title: "Zephyr: West Flash", category: "Zephyr", run: () => void west("flash") });
registerCommand({ id: "cpp.zephyr.debug", title: "Zephyr: West Debug", category: "Zephyr", run: () => void west("debug") });
registerCommand({ id: "cpp.zephyr.board", title: "Zephyr: Select Board…", category: "Zephyr", run: () => chooseBoard() });

registerMainMenuEntries(
  "Tools",
  [
    group("ESP-_IDF", [
      action("cpp.espidf.build", "_Build"),
      action("cpp.espidf.flash", "_Flash"),
      action("cpp.espidf.monitor", "_Monitor"),
      action("cpp.espidf.menuconfig", "Menu_config"),
    ]),
  ],
  { when: idfActive },
);
registerMainMenuEntries(
  "Tools",
  [
    group("_Zephyr", [
      action("cpp.zephyr.build", "West _Build"),
      action("cpp.zephyr.flash", "West _Flash"),
      action("cpp.zephyr.debug", "West _Debug"),
      action("cpp.zephyr.board", "Select B_oard..."),
    ]),
  ],
  { when: zephyrActive },
);

registerSettingsPage({
  id: "build.embedded",
  title: t("settings.page.build.embedded"),
  group: "Build, Execution, Deployment",
  order: 6,
  keywords: ["openocd", "platformio", "pio", "esp-idf", "idf.py", "zephyr", "west", "board", "svd", "gdb", "serial port"],
  component: () => <SchemaPage pageId="build.embedded" />,
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() =>
    createEffect(
      on(
        () => [!!workspace() && cppActive(), workspace()?.root] as const,
        ([active]) => {
          if (active) void refreshEmbedded();
          else setInfo(null);
        },
      ),
    ),
  );
}
