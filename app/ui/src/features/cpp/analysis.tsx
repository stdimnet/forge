/**
 * CForge's dynamic analysis and package managers in the UI:
 *
 * - the Sanitizers and Valgrind tabs of the Run tool window (`registerRunPanel`) and the
 *   editor highlights of the selected problem,
 * - Run | Run with Valgrind Memcheck and Run | Profile with perf (the selected CMake
 *   Application, Google Test or Catch2 configuration, `RunRequest.nativeTool`), with perf
 *   profiles in the shared Profiler tool window,
 * - Conan Install for the active CMake profile, vcpkg port completion in `vcpkg.json`,
 * - Settings | Build, Execution, Deployment | Dynamic Analysis Tools and Package Managers.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { ShieldAlert, Bug } from "lucide-solid";
import { cppToolsApi, type NativeTool, type PackageManagers } from "@/ipc/cppTools";
import type { RunConfiguration } from "@/ipc/run";
import { registerEditorExtension } from "@/editor/extensions";
import { registerKeymapActions } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { registerProfilerSource } from "@/features/profiler";
import { launch, onRunEvent, registerRunPanel } from "@/features/run/runStore";
import { selectedConfiguration } from "@/features/run/configStore";
import { openRunConfigurations } from "@/features/run/dialogState";
import { openSettings } from "@/features/settings/dialogState";
import { isDialogWindow } from "@/shell/window/mode";
import { registerCommand } from "@/state/commands";
import { workspace } from "@/state/workspace";
import { acceptSanitizer, acceptValgrind, SanitizerPanel, ValgrindPanel } from "./AnalysisPanels";
import { analysisHighlightExtension } from "./analysisHighlights";
import { PerfView, setPerfUnsupported, showPerfProfile } from "./PerfView";
import { cppActive, refreshProfiles, reloadCMake, showCppBalloon } from "./state";
import "./analysis.css";
import { t } from "@/i18n";

// ---- Result panels and the Profiler ---------------------------------------------------------

registerRunPanel({
  id: "sanitizers",
  title: t("langs.cpp.sanitizers"),
  icon: ShieldAlert,
  accepts: acceptSanitizer,
  count: (d) => (d as unknown[]).length,
  reveal: () => true,
  component: SanitizerPanel,
});

registerRunPanel({
  id: "valgrind",
  title: t("langs.cpp.valgrind"),
  icon: Bug,
  accepts: acceptValgrind,
  count: (d) => (d as { errors: unknown[] }).errors.length,
  reveal: () => true,
  component: ValgrindPanel,
});

onRunEvent((_s, e) => {
  if (e.type === "perf" && e.profile) showPerfProfile(e.profile);
});

registerProfilerSource({ id: "perf", label: "perf", active: cppActive, component: PerfView, order: 30 });

registerEditorExtension(({ path }) => (/\.(c|cc|cpp|cxx|c\+\+|h|hh|hpp|hxx|ipp|inl|cu)$/i.test(path) ? analysisHighlightExtension(path) : null));

// ---- Run with Valgrind Memcheck / Profile with perf -----------------------------------------

const NATIVE_TYPES = new Set(["cmake-app", "gtest", "catch2"]);
const LABEL: Record<NativeTool, string> = { valgrind: "Valgrind Memcheck", perf: "perf" };

async function runWith(tool: NativeTool, cfg: RunConfiguration | undefined = selectedConfiguration()): Promise<void> {
  if (!cfg) return openRunConfigurations();
  if (!NATIVE_TYPES.has(cfg.type)) {
    showCppBalloon(`Cannot run '${cfg.name}' with ${LABEL[tool]}`, "Choose a CMake Application, Google Test or Catch2 configuration.", 6000);
    return;
  }
  const info = await cppToolsApi.nativeTools().catch(() => null);
  const why = tool === "perf" ? info?.perfUnsupported : info?.valgrindUnsupported;
  if (why) {
    setPerfUnsupported(tool === "perf" ? why : null);
    showCppBalloon(tool === "perf" ? "perf is not available" : "Valgrind is not available", why, 8000);
    return;
  }
  if (info && !(tool === "perf" ? info.perf : info.valgrind)) {
    showCppBalloon(`${tool === "perf" ? "perf" : "Valgrind"} is not installed`, `Install it with your package manager or choose it in Settings | Build, Execution, Deployment | Dynamic Analysis Tools.`, 8000, [
      { label: t("langs.cpp.openSettings"), run: () => openSettings("build.analysis") },
    ]);
    return;
  }
  await launch(cfg, { extra: { nativeTool: tool } });
}

// ---- Packages -------------------------------------------------------------------------------

const [packages, setPackages] = createSignal<PackageManagers | null>(null);
export { packages as cppPackages };

export async function refreshPackages(): Promise<PackageManagers | null> {
  const p = await cppToolsApi.packages().catch(() => null);
  setPackages(p);
  return p;
}

/** Conan Install for the active profile, then reload CMake with Conan's toolchain file. */
async function conanInstall(): Promise<void> {
  try {
    const cmd = await cppToolsApi.conanInstallCommand();
    const handle = await launch({ name: "Conan Install", type: "cpp-tool", options: { commandLine: JSON.stringify(cmd) } });
    if (!handle || (await handle.exited) !== 0) return;
    await refreshPackages();
    await refreshProfiles();
    await reloadCMake();
  } catch (e) {
    showCppBalloon("Cannot run Conan", String(e).replace(/^Error: /, ""), 8000);
  }
}

// ---- Actions, menus and settings ------------------------------------------------------------

registerKeymapActions([
  { id: "cpp.runValgrind", title: "Run with Valgrind Memcheck", group: "Main Menu/Run" },
  { id: "cpp.profilePerf", title: "Profile with perf", group: "Main Menu/Run" },
  { id: "cpp.conanInstall", title: "Conan Install", group: "Plugins/C/C++" },
]);

registerCommand({ id: "cpp.runValgrind", title: "Run with Valgrind Memcheck", category: "Run", run: () => void runWith("valgrind") });
registerCommand({ id: "cpp.profilePerf", title: "Profile with perf", category: "Run", run: () => void runWith("perf") });
registerCommand({ id: "cpp.conanInstall", title: "Conan Install", category: "C/C++", run: () => void conanInstall() });

registerMainMenuEntries("Run", [action("cpp.runValgrind", "Run with _Valgrind Memcheck"), action("cpp.profilePerf", "Profile with _perf")], {
  after: "Debug",
  when: cppActive,
});
registerMainMenuEntries("Tools", [action("cpp.conanInstall", "Conan _Install")], { when: () => cppActive() && !!packages()?.conan });

registerSettingsPage({
  id: "build.analysis",
  title: t("settings.page.build.analysis"),
  group: "Build, Execution, Deployment",
  order: 4,
  keywords: ["valgrind", "memcheck", "perf", "profiler", "sanitizer", "flame graph"],
  component: () => <SchemaPage pageId="build.analysis" />,
});

registerSettingsPage({
  id: "build.packages",
  title: t("settings.page.build.packages"),
  group: "Build, Execution, Deployment",
  order: 5,
  keywords: ["vcpkg", "conan", "toolchain file", "VCPKG_ROOT", "conan_toolchain.cmake"],
  component: () => <SchemaPage pageId="build.packages" />,
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() =>
    createEffect(
      on(
        () => [!!workspace() && cppActive(), workspace()?.root] as const,
        ([active]) => {
          if (!active) return;
          void refreshPackages();
          void cppToolsApi.nativeTools().then((i) => setPerfUnsupported(i.perfUnsupported ?? null)).catch(() => undefined);
        },
      ),
    ),
  );
}
