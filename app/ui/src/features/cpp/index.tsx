/**
 * CForge in the UI (CLion's C/C++ support). Everything is computed in Rust (`src/ipc/cpp.ts`,
 * crates `packs/cforge/*`); this module plugs it into the contribution points:
 *
 * - Settings | Build, Execution, Deployment | Toolchains and CMake (profiles and presets),
 *   Settings | Languages & Frameworks | C/C++ (clangd, clang-format, debugger renderers),
 * - the CMake tool window (profiles, targets, configure log) and the Problems tool window's
 *   Build tab (compiler and linker messages of the last build),
 * - Build Project (`CompileDirty`, Ctrl+F9), Rebuild, Clean, Build Target, Reload CMake
 *   Project and Reset Cache, Download clangd,
 * - the "CMake Application" run configuration type and the manifest's Google Test, Catch2
 *   and CTest types, which build their target first; Debug through the toolchain's LLDB or
 *   GDB with STL renderers (`cpp_debug_request`, `dap_start`),
 * - CMake completion, clang-format and reload on change (`editor.ts`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { AppWindow, Hammer, Triangle } from "lucide-solid";
import { cppApi } from "@/ipc/cpp";
import { runApi, type RunConfiguration, type RunEvent } from "@/ipc/run";
import { registerKeymapActions } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerToolWindow } from "@/features/registry";
import { registerProblemsTab } from "@/features/lsp/problemsTabs";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { dapApi } from "@/features/debug/dapApi";
import { setInputDialog } from "@/features/git/overlays";
import { baseName, registerDebugLauncher, registerRunConfigurationType, testFrameworkType } from "@/features/run/types";
import { isDialogWindow } from "@/shell/window/mode";
import { commands, registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { getSetting, settingsState } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { safeMode } from "@/features/trust/store";
import { BUILD_TAB, BuildProblemsView, buildProblems, publishBuildOutput } from "./BuildProblems";
import { CMakeToolWindow } from "./CMakeToolWindow";
import { CppSettingsPage } from "./CppSettingsPage";
import { ProfilesPage } from "./ProfilesPage";
import { ToolchainsPage } from "./ToolchainsPage";
import { installClangd, offerClangd } from "./clangd";
import "./editor";
import {
  cmakeState,
  cppActive,
  executableTargets,
  PLUGIN,
  refreshCMake,
  refreshCppStatus,
  refreshProfiles,
  reloadCMake,
  runBuild,
  showCppBalloon,
} from "./state";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";

// ---- Settings ---------------------------------------------------------------------------------

registerSettingsPage({
  id: "build.toolchains",
  title: "Toolchains",
  group: "Build, Execution, Deployment",
  order: 2,
  keywords: ["toolchain", "gcc", "g++", "clang", "msvc", "visual studio", "mingw", "cross compiler", "cmake", "ninja", "make", "gdb", "lldb", "debugger", "compiler"],
  component: ToolchainsPage,
});

registerSettingsPage({
  id: "build.cmake",
  title: "CMake",
  group: "Build, Execution, Deployment",
  order: 3,
  keywords: ["cmake", "profile", "preset", "CMakePresets.json", "build type", "debug", "release", "generator", "build directory", "cmake options"],
  component: ProfilesPage,
});

registerSettingsPage({
  id: "lang.cpp",
  title: "C/C++",
  group: "Languages & Frameworks",
  order: 27,
  keywords: ["c", "c++", "cpp", "clangd", "clang-tidy", "clang-format", "compile_commands.json", "compilation database", "pretty printers", "stl"],
  component: CppSettingsPage,
});

// ---- Tool windows -------------------------------------------------------------------------------

registerToolWindow({
  id: "cmake",
  title: "CMake",
  icon: Triangle,
  anchor: "bottom",
  order: 45,
  component: CMakeToolWindow,
  when: () => cppActive() && cmakeState()?.status !== "noCMake",
});

registerProblemsTab({
  id: BUILD_TAB,
  title: "Build",
  order: 5,
  count: () => buildProblems().filter((p) => p.severity === "error").length,
  visible: cppActive,
  component: BuildProblemsView,
});

// ---- Run configurations -----------------------------------------------------------------------

/** Build a configuration's CMake target first (`cmake.buildBeforeRun`); false when it failed. */
async function buildFirst(cfg: RunConfiguration): Promise<boolean> {
  if (!getSetting<boolean>("cmake.buildBeforeRun", true)) return true;
  if (cfg.type === "ctest") {
    return (await runBuild("build", undefined, "Build All")) === 0;
  }
  const target = await cppApi.targetOf(cfg).catch(() => null);
  if (!target) return true;
  return (await runBuild("build", [target], `Build ${target}`)) === 0;
}

registerRunConfigurationType({
  id: "cmake-app",
  name: "CMake Application",
  description: "Build a CMake executable target and run or debug it (LLDB or GDB of the profile's toolchain).",
  icon: AppWindow,
  color: "var(--syn-function)",
  pluginId: PLUGIN,
  argumentsLabel: "Program arguments",
  debuggable: true,
  noRunTarget: true,
  defaults: () => ({ options: { target: executableTargets()[0]?.name ?? "" } }),
  fields: [
    {
      key: "target",
      label: "Target",
      kind: "enum",
      required: true,
      options: () => executableTargets().map((t) => ({ value: t.name, label: t.name })),
    },
    { key: "executable", label: "Executable", kind: "path", placeholder: "Empty: the target's executable" },
    {
      key: "debugger",
      label: "Debugger",
      kind: "enum",
      default: "",
      options: [
        { value: "", label: "Toolchain default" },
        { value: "gdb", label: "GDB" },
        { value: "lldb", label: "LLDB" },
      ],
    },
  ],
  suggestName: (cfg) => opt(cfg, "target") || baseName(opt(cfg, "executable")),
  start: async (cfg, request) => {
    if (!(await buildFirst(cfg))) throw new Error(`Building '${opt(cfg, "target")}' failed`);
    return runApi.start(request);
  },
});

let buildSeq = 0;
/** A build command line in the Run tool window; its output becomes the Build problems. */
registerRunConfigurationType({
  id: "cpp-build",
  name: "CMake Build",
  icon: Hammer,
  hidden: true,
  fields: [],
  argumentsLabel: false,
  buildCommand: (cfg) => JSON.parse(cfg.options?.commandLine ?? "{}"),
  start: async (_cfg, request) => {
    const runId = request.runId ?? `cpp-build-${++buildSeq}`;
    const chunks: string[] = [];
    const off = await runApi.onEvent(runId, (e: RunEvent) => {
      if (e.type === "output") chunks.push(e.text);
      if (e.type === "exited") {
        off();
        void publishBuildOutput(chunks.join(""), e.code ?? null);
      }
    });
    return runApi.start({ ...request, runId });
  },
});

// Google Test, Catch2 and CTest from the manifest's `testFrameworks`, building first.
const TEST_TYPES = ["gtest", "catch2", "ctest"];
if (typeof document !== "undefined") {
  createRoot(() =>
    createEffect(
      on(
        () => settingsState.plugins.find((p) => p.id === PLUGIN)?.contributions?.testFrameworks?.filter((f) => TEST_TYPES.includes(f.id)),
        (specs) => {
          for (const spec of specs ?? []) {
            const base = testFrameworkType(spec, PLUGIN);
            registerRunConfigurationType({
              ...base,
              color: "var(--success)",
              fields: base.fields.filter((f) => !["testRunnerOptions", "directory"].includes(f.key)),
              argumentsLabel: spec.id === "ctest" ? "CTest arguments" : "Program arguments",
              start: async (cfg, request) => {
                if (!(await buildFirst(cfg))) throw new Error("The build failed: see the Build problems");
                return runApi.start(request);
              },
            });
          }
        },
      ),
    ),
  );
}

// ---- Debugging ----------------------------------------------------------------------------------

const DEBUG_TYPES = new Set(["cmake-app", "gtest", "catch2"]);

registerDebugLauncher({
  id: "cforge",
  canDebug: (cfg) => DEBUG_TYPES.has(cfg.type) && cppActive(),
  async launch(cfg) {
    try {
      if (!(await buildFirst(cfg))) return;
      const request = await cppApi.debugRequest(cfg);
      await refreshDebugAdapters();
      showToolWindow("debug");
      await dapApi.start(request);
    } catch (e) {
      showCppBalloon(`Cannot debug '${cfg.name}'`, String(e).replace(/^Error: /, ""), 8000);
    }
  },
});

// ---- Actions ------------------------------------------------------------------------------------

function buildTarget(): void {
  const targets = executableTargets().map((t) => t.name);
  setInputDialog({
    title: "Build Target",
    label: `CMake target${targets.length ? ` (${targets.join(", ")})` : ""}:`,
    value: targets[0] ?? "",
    okLabel: "Build",
    onOk: (value) => void runBuild("build", [value.trim()]),
  });
}

registerKeymapActions([
  { id: "cmake.reload", title: "Reload CMake Project", group: "Plugins/C/C++" },
  { id: "cmake.resetCache", title: "Reset Cache and Reload Project", group: "Plugins/C/C++" },
  { id: "cmake.rebuild", title: "Rebuild Project", group: "Plugins/C/C++" },
  { id: "cmake.clean", title: "Clean", group: "Plugins/C/C++" },
  { id: "cmake.buildTarget", title: "Build Target…", group: "Plugins/C/C++" },
  { id: "cpp.installClangd", title: "Download clangd", group: "Plugins/C/C++" },
]);

const needsCpp = (run: () => void) => () => {
  if (!cppActive()) {
    showCppBalloon("No C/C++ project", "Open a project with a CMakeLists.txt or compile_commands.json, or enable the C/C++ plugin.", 5000);
    return;
  }
  run();
};

// Build Project (Ctrl+F9) builds the CMake project in C/C++ projects and keeps the other
// packs' Build Project (JavaForge) elsewhere.
const otherBuild = commands().get("CompileDirty");
registerCommand({
  id: "CompileDirty",
  title: "Build Project",
  category: "Build",
  run: () => {
    if (cppActive()) void runBuild("build");
    else if (otherBuild) void otherBuild.run();
    else showCppBalloon("Nothing to build", "The project has no build Forge knows.", 4000);
  },
});
registerCommand({ id: "cmake.reload", title: "Reload CMake Project", category: "CMake", run: needsCpp(() => void reloadCMake()) });
registerCommand({ id: "cmake.resetCache", title: "Reset Cache and Reload Project", category: "CMake", run: needsCpp(() => void reloadCMake(true)) });
registerCommand({ id: "cmake.rebuild", title: "Rebuild Project", category: "Build", run: needsCpp(() => void runBuild("rebuild")) });
registerCommand({ id: "cmake.clean", title: "Clean", category: "Build", run: needsCpp(() => void runBuild("clean")) });
registerCommand({ id: "cmake.buildTarget", title: "Build Target…", category: "Build", run: needsCpp(buildTarget) });
registerCommand({ id: "cpp.installClangd", title: "Download clangd", category: "C/C++", run: () => void installClangd() });

registerMainMenuEntries(
  "_Build",
  [
    action("CompileDirty", "_Build Project"),
    action("cmake.rebuild", "_Rebuild Project"),
    action("cmake.clean", "_Clean"),
    action("cmake.buildTarget", "Build _Target…"),
    action("cmake.reload", "Reload CMake Pro_ject"),
  ],
  { when: cppActive },
);

// ---- Startup ------------------------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [!!workspace() && cppActive(), workspace()?.root] as const,
        ([active]) => {
          if (!active) return;
          void refreshProfiles();
          void refreshCppStatus().then((s) => {
            if (s && !s.remote && !s.clangd.path) offerClangd();
          });
          void refreshCMake().then((s) => {
            // CLion loads the CMake project when it opens (CMake runs project code: not in
            // Safe Mode).
            if (s?.status === "notLoaded" && !safeMode()) void reloadCMake();
          });
        },
      ),
    );
  });
}
