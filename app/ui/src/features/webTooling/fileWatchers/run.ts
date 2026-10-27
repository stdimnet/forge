/**
 * Running File Watchers: after a file is saved (and, for watchers with "Trigger the watcher
 * on external changes", after files change on disk) Rust plans the runs
 * (`file_watchers_plan`: file type, scope, "Track only root files", macros) and they start
 * in the Run tool window as hidden `file-watcher` configurations. The console opens as
 * the watcher's "Show console" says; problems found by its output filters
 * (`file_watchers_problems`) go to the Problems tool window's "File Watchers" tab.
 * Watchers with "Auto-save edited files to trigger the watcher" save the file after a
 * pause in typing. Refused in Safe Mode (the plan command fails).
 */
import { createSignal } from "solid-js";
import { Eye } from "lucide-solid";
import { runApi, type RunConfiguration } from "@/ipc/run";
import { fileWatchersApi, watcherFromObject, type FileWatcher, type WatcherProblem, type WatcherRun, type WatcherTrigger } from "@/ipc/webTooling";
import { launch, registerRunConfigurationType } from "@/features/run";
import { refreshDirs } from "@/features/project/ProjectView";
import { showToolWindow } from "@/state/layout";
import { getSetting } from "@/state/settings";
import { balloonsFor } from "@/keymap";

const showBalloon = balloonsFor("File Watchers");

export const WATCHERS_KEY = "tools.fileWatchers";
export const WATCHER_TYPE = "file-watcher";

/** The configured watchers (project setting). */
export const fileWatchers = (): FileWatcher[] => {
  const v: unknown = getSetting(WATCHERS_KEY, []);
  return Array.isArray(v) ? (v as Record<string, unknown>[]).filter((o) => o && typeof o === "object").map(watcherFromObject) : [];
};

const parse = <T>(text: string | undefined, fallback: T): T => {
  try {
    return text ? (JSON.parse(text) as T) : fallback;
  } catch {
    return fallback;
  }
};

registerRunConfigurationType({
  id: WATCHER_TYPE,
  name: "File Watcher",
  icon: Eye,
  color: "var(--fg-muted)",
  fields: [],
  hidden: true,
  buildCommand: (cfg) => ({
    program: cfg.options?.program ?? "",
    args: parse<string[]>(cfg.options?.args, []),
    cwd: cfg.options?.cwd ?? "",
    env: parse<Record<string, string>>(cfg.options?.env, {}),
  }),
  outputFilters: (cfg) => parse<string[]>(cfg.options?.outputFilters, []),
});

// ---- Problems ------------------------------------------------------------------------------

export interface WatcherProblems {
  watcher: string;
  file: string;
  problems: WatcherProblem[];
}

const [problemSets, setProblemSets] = createSignal<WatcherProblems[]>([]);
export { problemSets as watcherProblems };
export const watcherProblemCount = () => problemSets().reduce((n, s) => n + s.problems.length, 0);

function setProblems(watcher: string, file: string, problems: WatcherProblem[]) {
  setProblemSets((prev) => {
    const rest = prev.filter((p) => !(p.watcher === watcher && p.file === file));
    return problems.length ? [...rest, { watcher, file, problems }] : rest;
  });
}

export const clearWatcherProblems = () => setProblemSets([]);

// ---- Runs ----------------------------------------------------------------------------------

const running = new Set<string>();

async function start(run: WatcherRun): Promise<void> {
  const key = `${run.watcher}\u0000${run.file}`;
  if (running.has(key)) return;
  running.add(key);
  try {
    const cfg: RunConfiguration = {
      name: `${run.watcher}: ${run.file.split("/").pop()}`,
      type: WATCHER_TYPE,
      options: {
        program: run.program,
        args: JSON.stringify(run.args),
        cwd: run.cwd,
        env: JSON.stringify(run.env),
        outputFilters: JSON.stringify(run.outputFilters),
      },
    };
    const handle = await launch(cfg, { background: run.showConsole !== "always" });
    if (!handle) return;
    let output = "";
    let stderr = false;
    const stop = await runApi.onEvent(handle.started.runId, (e) => {
      if (e.type !== "output") return;
      output += e.text;
      if (e.stream === "stderr") stderr = true;
    });
    const code = await handle.exited;
    stop();
    if (run.showConsole === "error" && (code !== 0 || stderr)) showToolWindow("run");
    const problems = run.outputFilters.length ? await fileWatchersApi.problems(output, run.outputFilters, run.cwd).catch(() => []) : [];
    setProblems(run.watcher, run.file, problems);
    // Show the generated files in the Project view.
    const dir = run.file.includes("/") ? run.file.slice(0, run.file.lastIndexOf("/")) : "";
    refreshDirs(dir ? ["", dir] : [""]);
  } finally {
    running.delete(key);
  }
}

/** Plan and start the watchers for files that were saved or changed. */
export async function triggerWatchers(paths: string[], trigger: WatcherTrigger): Promise<WatcherRun[]> {
  const relevant = paths.filter((p) => !p.includes("://"));
  if (!relevant.length || !fileWatchers().some((w) => w.enabled)) return [];
  let runs: WatcherRun[];
  try {
    runs = await fileWatchersApi.plan(relevant, trigger);
  } catch (err) {
    showBalloon("File Watchers did not run", String(err));
    return [];
  }
  for (const r of runs) void start(r);
  return runs;
}

/**
 * "Auto-save edited files to trigger the watcher": after a pause in typing, ask Rust which
 * auto-save watchers apply to the file (trigger `change`); when any does, the file is
 * saved, which runs the watchers like a manual save.
 */
export async function autoSaveWanted(path: string): Promise<boolean> {
  if (!fileWatchers().some((w) => w.enabled && w.autoSave)) return false;
  const runs = await fileWatchersApi.plan([path], "change").catch(() => []);
  return runs.length > 0;
}
