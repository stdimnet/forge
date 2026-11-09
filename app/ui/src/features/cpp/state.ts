/**
 * CForge state in the UI: whether the C/C++ plugin is active, the CMake model of the active
 * profile (`cmake_state`, `cmake://state`), the configure log (`cmake://log`), the profiles
 * and the status of clangd and the toolchain. Everything is computed in Rust (`@/ipc/cpp`).
 */
import { createSignal } from "solid-js";
import { cppApi, type BuildAction, type CMakeState, type CMakeTarget, type CppStatus, type ProfilesInfo } from "@/ipc/cpp";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { activeSessionId, launch, setActiveSession } from "@/features/run/runStore";
import { getSetting } from "@/state/settings";
import { balloonsFor } from "@/keymap/dispatch";
import { t } from "@/i18n";
import { isPluginActive } from "@/state/project";

export const PLUGIN = "forge.cpp";
export const cppActive = () => isPluginActive(PLUGIN);
export const showCppBalloon = balloonsFor("C/C++");

const [cmake, setCmake] = createSignal<CMakeState | null>(null);
const [log, setLog] = createSignal<{ line: string; stream: string }[]>([]);
const [profiles, setProfiles] = createSignal<ProfilesInfo | null>(null);
const [status, setStatus] = createSignal<CppStatus | null>(null);
export { cmake as cmakeState, log as cmakeLog, profiles as cmakeProfiles, status as cppStatus };

let listening = false;
async function listenOnce(): Promise<void> {
  if (listening) return;
  listening = true;
  await cppApi.onState((s) => setCmake(s));
  await cppApi.onLog((e) => setLog((l) => [...l.slice(-4999), e]));
}

/** The model a configure wrote earlier (no CMake run). */
export async function refreshCMake(): Promise<CMakeState | null> {
  await listenOnce();
  try {
    const s = await cppApi.state();
    setCmake(s);
    if (!log().length && s.log.length) setLog(s.log.map((line) => ({ line, stream: "stdout" })));
    return s;
  } catch {
    return null;
  }
}

export async function refreshProfiles(): Promise<ProfilesInfo | null> {
  try {
    const p = await cppApi.profiles();
    setProfiles(p);
    return p;
  } catch {
    return null;
  }
}

export async function refreshCppStatus(refresh = false): Promise<CppStatus | null> {
  try {
    const s = await cppApi.status(refresh);
    setStatus(s);
    return s;
  } catch {
    return null;
  }
}

/** The balloon text of a failed CMake configure with `errors` error messages (exported for tests). */
export function cmakeFailureDetail(errors: number): string {
  return errors ? t("langs.cpp.cmakeErrorsSeeTheToolWindow", { count: errors }) : t("langs.cpp.seeTheCmakeToolWindow");
}

/** Reload CMake Project (Reset Cache and Reload Project with `resetCache`). */
export async function reloadCMake(resetCache = false): Promise<CMakeState | null> {
  await listenOnce();
  setLog([]);
  setCmake((s) => (s ? { ...s, status: "loading" } : s));
  try {
    const s = await cppApi.reload(resetCache);
    setCmake(s);
    const errors = s.messages.filter((m) => m.severity === "error").length;
    if (s.status === "failed") showCppBalloon(t("langs.cpp.cmakeConfigureFailed"), cmakeFailureDetail(errors), 6000);
    return s;
  } catch (e) {
    setCmake((s) => (s ? { ...s, status: "failed" } : s));
    showCppBalloon(t("langs.cpp.cannotReloadTheCmakeProject"), String(e).replace(/^Error: /, ""), 8000);
    return null;
  }
}

/** The targets of the active profile's configuration. */
export function cmakeTargets(): CMakeTarget[] {
  const s = cmake();
  const p = profiles()?.profiles.find((x) => x.name === s?.profile);
  const configs = s?.model?.configurations ?? [];
  const cfg = configs.find((c) => c.name.toLowerCase() === (p?.buildType ?? "").toLowerCase()) ?? configs[0];
  return (cfg?.targets ?? []).filter((t) => !t.generatorProvided);
}

export const executableTargets = () => cmakeTargets().filter((t) => t.kind === "executable");

/** `cmake --build …` for a command line in the Run tool window. */
export function commandName(cmd: CommandLine): string {
  const program = cmd.program.replace(/\\/g, "/").split("/").pop() ?? cmd.program;
  const line = [program, ...cmd.args].join(" ");
  return line.length > 70 ? `${line.slice(0, 67)}…` : line;
}

/** Build, Rebuild or Clean targets (all without targets); resolves with the exit code. */
export async function runBuild(action: BuildAction, targets?: string[], name?: string): Promise<number | null> {
  try {
    const s = cmake() ?? (await refreshCMake());
    if (s && s.status === "notLoaded") {
      const loaded = await reloadCMake();
      if (!loaded || loaded.status !== "loaded") return 1;
    }
    const cmd = await cppApi.buildCommand(action, targets);
    const label = name ?? (action === "clean" ? "Clean" : `${action === "rebuild" ? "Rebuild" : "Build"} ${targets?.length ? targets.join(", ") : "All"}`);
    const cfg: RunConfiguration = { name: label, type: "cpp-build", options: { commandLine: JSON.stringify(cmd) } };
    const handle = await launch(cfg);
    return handle ? await handle.exited : null;
  } catch (e) {
    showCppBalloon("Cannot build", String(e).replace(/^Error: /, ""), 8000);
    return null;
  }
}

/** Build a configuration's CMake target first (`cmake.buildBeforeRun`); false when it failed. */
export async function buildFirst(cfg: RunConfiguration): Promise<boolean> {
  if (!getSetting<boolean>("cmake.buildBeforeRun", true)) return true;
  const target = cfg.type === "ctest" ? null : await cppApi.targetOf(cfg).catch(() => null);
  if (cfg.type !== "ctest" && !target) return true;
  // The run's own tab comes back to the front once its target is built.
  const run = activeSessionId();
  const code = await runBuild("build", target ? [target] : undefined, target ? `Build ${target}` : "Build All");
  if (code === 0 && run) setActiveSession(run);
  return code === 0;
}

/** Run a command line in the Run tool window (Conan, PlatformIO, idf.py, west, OpenOCD); resolves with the run. */
export function runTool(name: string, cmd: CommandLine) {
  return launch({ name, type: "cpp-tool", options: { commandLine: JSON.stringify(cmd) } });
}
