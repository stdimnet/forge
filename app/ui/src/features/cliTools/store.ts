/**
 * Command Line Tools Console state: the project's tools, their commands (loaded once per
 * console session and on "Reload"), the input history and the runs. A run streams the
 * backend's `run://` events; its console lines (ANSI styles, file links) are computed in
 * Rust like the Run tool window's.
 */
import { batch, createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { cliToolsApi, toolName, type CliTool, type ShowConsoleIn, type ToolCommands } from "@/ipc/cliTools";
import { runApi, type ConsoleLine, type RunEvent } from "@/ipc/run";
import { applyConsoleDelta } from "@/features/run/tests";
import { balloonsFor } from "@/keymap";
import { saveAll } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { isPluginActive } from "@/state/project";

export const TOOL_WINDOW_ID = "commandLineTools";
export const PLUGIN_ID = "forge.command-line-tools";
export const TOOLS_SETTING = "commandLineTools.tools";

const showBalloon = balloonsFor("Command Line Tools");

export const cliToolsActive = () => isPluginActive(PLUGIN_ID);

// ---- Tools and commands --------------------------------------------------------------

const [tools, setTools] = createSignal<CliTool[]>([]);
const [toolsLoaded, setToolsLoaded] = createSignal(false);
export { tools, toolsLoaded };

export const enabledTools = () => tools().filter((t) => t.enabled);

export async function loadTools(): Promise<CliTool[]> {
  try {
    const list = await cliToolsApi.list();
    setTools(list);
    return list;
  } catch (err) {
    showBalloon("Cannot load command line tools", String(err));
    return tools();
  } finally {
    setToolsLoaded(true);
  }
}

/** Commands per alias, with `loading` while a list runs. */
const [commandState, setCommandState] = createStore<Record<string, (ToolCommands & { loading?: boolean }) | undefined>>({});
export { commandState };

/** Load (or with `reload`, list again) the commands of a tool. */
export async function loadCommands(alias: string, reload = false): Promise<void> {
  if (commandState[alias]?.loading) return;
  setCommandState(alias, { alias, commands: commandState[alias]?.commands ?? [], source: "none", loading: true });
  try {
    setCommandState(alias, { ...(await cliToolsApi.commands(alias, reload)), loading: false });
  } catch (err) {
    setCommandState(alias, { alias, commands: [], source: "none", error: String(err), loading: false });
  }
}

/** Reload the tool list and every tool's commands (the console's "Reload" button). */
export async function reloadAll(): Promise<void> {
  const list = await loadTools();
  await Promise.all(list.filter((t) => t.enabled).map((t) => loadCommands(t.alias, true)));
}

/** Tools first, then their commands (or from the cache) when a console opens. */
export async function prepareConsole(): Promise<void> {
  const list = await loadTools();
  await Promise.all(list.filter((t) => t.enabled && !commandState[t.alias]).map((t) => loadCommands(t.alias)));
}

// ---- History -------------------------------------------------------------------------

const MAX_HISTORY = 100;
/** Newest first. */
const [history, setHistory] = createSignal<string[]>([]);
export { history };

export function remember(input: string): void {
  const text = input.trim();
  if (text) setHistory((h) => [text, ...h.filter((x) => x !== text)].slice(0, MAX_HISTORY));
}

// ---- Runs ----------------------------------------------------------------------------

export type ConsoleStatus = "starting" | "running" | "finished" | "failed";

export interface ConsoleRun {
  id: string;
  runId: string | null;
  input: string;
  commandLine: string;
  alias: string;
  tool: string;
  target: ShowConsoleIn;
  status: ConsoleStatus;
  exitCode?: number | null;
  /** Bumped when the lines change (they are mutated in place). */
  version: number;
}

const [state, setState] = createStore<{ runs: ConsoleRun[]; active: string | null }>({ runs: [], active: null });
const lines = new Map<string, ConsoleLine[]>();
const unlisten = new Map<string, () => void>();
let seq = 0;

export const consoleRuns = () => state.runs;
export const activeRunId = () => state.active;
export const activeRun = () => state.runs.find((r) => r.id === state.active);
export const findRun = (id: string | null | undefined) => state.runs.find((r) => r.id === id);
export const setActiveRun = (id: string) => setState("active", id);
export const runLines = (id: string) => lines.get(id) ?? [];
export const isLive = (r: ConsoleRun | undefined) => !!r && (r.status === "starting" || r.status === "running");

const update = (id: string, patch: Partial<ConsoleRun>) => setState("runs", (r) => r.id === id, patch);
const bump = (id: string) => setState("runs", (r) => r.id === id, "version", (v) => v + 1);

function writeError(id: string, text: string): void {
  lines.get(id)?.push({ segments: [{ text, style: { fg: "var(--danger)" } }] });
  bump(id);
}

function onEvent(id: string, runId: string, e: RunEvent): void {
  const run = findRun(id);
  if (!run || run.runId !== runId) return;
  switch (e.type) {
    case "started":
      update(id, { status: "running", commandLine: e.commandLine });
      break;
    case "console":
      applyConsoleDelta(lines.get(id)!, e);
      bump(id);
      break;
    case "exited":
      update(id, { status: "finished", exitCode: e.code ?? (e.signal ? 128 + e.signal : null) });
      unlisten.get(id)?.();
      unlisten.delete(id);
      break;
  }
}

/** Run `input` in tab `id` (a new process each time). */
async function start(id: string): Promise<void> {
  const run = findRun(id)!;
  unlisten.get(id)?.();
  const runId = `cli-${Date.now().toString(36)}-${++seq}`;
  lines.set(id, []);
  update(id, { runId, status: "starting", exitCode: undefined, version: run.version + 1 });
  unlisten.set(id, await runApi.onEvent(runId, (e) => onEvent(id, runId, e)));
  try {
    const started = await cliToolsApi.run(run.input, runId);
    // The target was chosen from the tool list when the run was created.
    update(id, { commandLine: started.commandLine, alias: started.alias, tool: started.tool });
  } catch (err) {
    unlisten.get(id)?.();
    unlisten.delete(id);
    writeError(id, String(err));
    update(id, { status: "failed" });
  }
}

/**
 * Run a console input (`alias command args`). Its output goes to the tool's console: a tab
 * of the Command Line Tools Console tool window, or the popup; `target` overrides the
 * tool's "Show console in" (input typed in the tool window stays there).
 */
export async function runInput(input: string, target?: ShowConsoleIn): Promise<ConsoleRun | null> {
  const text = input.trim();
  if (!text) return null;
  remember(text);
  await saveAll().catch(() => undefined);
  const alias = text.split(/\s+/)[0];
  const tool = tools().find((t) => t.alias === alias);
  const id = `cli-tab-${++seq}`;
  // Finished popup runs are dropped; tool window tabs stay until they are closed.
  batch(() => {
    setState(
      "runs",
      produce((list) => {
        for (let i = list.length - 1; i >= 0; i--) if (!isLive(list[i]) && list[i].target === "popup") list.splice(i, 1);
        list.push({
          id,
          runId: null,
          input: text,
          commandLine: "",
          alias,
          tool: tool ? toolName(tool) : alias,
          target: target ?? tool?.showConsoleIn ?? "toolWindow",
          status: "starting",
          version: 0,
        });
      }),
    );
    setState("active", id);
  });
  lines.set(id, []);
  await start(id);
  const run = findRun(id) ?? null;
  if (run?.target === "toolWindow") showToolWindow(TOOL_WINDOW_ID);
  return run;
}

export async function stopRun(id: string | null | undefined = state.active): Promise<void> {
  const run = findRun(id);
  if (!run?.runId || !isLive(run)) return;
  await runApi.stop(run.runId).catch((err) => showBalloon("Cannot stop the command", String(err)));
}

export async function rerun(id: string | null | undefined = state.active): Promise<void> {
  const run = findRun(id);
  if (!run) return;
  if (isLive(run) && run.runId) await runApi.stop(run.runId).catch(() => undefined);
  await saveAll().catch(() => undefined);
  await start(run.id);
}

export function clearRun(id: string | null | undefined = state.active): void {
  const l = id ? lines.get(id) : undefined;
  if (!l) return;
  l.length = 0;
  bump(id!);
}

export async function closeRun(id: string): Promise<void> {
  const run = findRun(id);
  if (!run) return;
  if (run.runId) await runApi.dispose(run.runId).catch(() => undefined);
  unlisten.get(id)?.();
  unlisten.delete(id);
  lines.delete(id);
  const idx = state.runs.findIndex((r) => r.id === id);
  batch(() => {
    setState("runs", (list) => list.filter((r) => r.id !== id));
    if (state.active === id) setState("active", state.runs[Math.min(idx, state.runs.length - 1)]?.id ?? null);
  });
}

/** Console input typed while a command runs (its stdin). */
export async function sendInput(id: string, text: string): Promise<void> {
  const run = findRun(id);
  if (!run?.runId || !isLive(run)) return;
  await runApi.write(run.runId, text).catch((err) => showBalloon("Cannot send input", String(err)));
}

/** Tool window tabs (popup runs are shown in the popup only). */
export const toolWindowRuns = () => state.runs.filter((r) => r.target === "toolWindow");

/** "Exit code 1", "Running…" */
export function statusText(run: ConsoleRun): string {
  if (isLive(run)) return "Running…";
  if (run.status === "failed") return "Failed to start";
  if (run.exitCode === 0) return "Finished";
  if (run.exitCode === null || run.exitCode === undefined) return "Stopped";
  return `Exit code ${run.exitCode}`;
}
