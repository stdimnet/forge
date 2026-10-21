/**
 * Runs shown in the Run tool window: one tab per run, its console and, for test runners, the
 * test tree (both computed by the backend and streamed as `console` / `tests` run events),
 * and Run / Debug / Stop / Rerun.
 */
import { batch, createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { runApi, type ConsoleLine, type RunConfiguration, type RunEvent, type RunRequest } from "@/ipc/run";
import { showBalloon } from "@/keymap";
import { saveAll } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { applyConsoleDelta, applyTestUpdates, emptyTestTree, type TestNode, type TestTreeState } from "./tests";
import { debugLauncherFor, findType, validateConfiguration, type RunHandle } from "./types";
import { findConfiguration, selectedConfiguration } from "./configStore";
import { openRunConfigurations } from "./dialogState";
import { publishTestResults } from "./testResults";

/**
 * Lines kept per console when the backend sends no cycle buffer trimming (UI messages only;
 * run output is trimmed in Rust by `run.console.cycleBufferKb`).
 */
const MAX_LINES = 100_000;

export type RunStatus = "starting" | "running" | "finished" | "failed";

export interface RunSession {
  /** Stable tab id; a rerun keeps it and gets a new `runId`. */
  id: string;
  runId: string | null;
  name: string;
  config: RunConfiguration;
  debug: boolean;
  status: RunStatus;
  exitCode?: number | null;
  startedAt: number;
  endedAt?: number;
  /** Bumped whenever console lines or the test tree change. */
  version: number;
  pinned: boolean;
}

interface SessionData {
  lines: ConsoleLine[];
  tests?: TestTreeState;
  exited?: (code: number | null) => void;
  unlisten?: () => void;
  extra?: Pick<RunRequest, "env" | "interpreterArgs">;
}

const [state, setState] = createStore<{ sessions: RunSession[]; active: string | null }>({
  sessions: [],
  active: null,
});
const data = new Map<string, SessionData>();
let seq = 0;

export const sessions = () => state.sessions;
export const activeSessionId = () => state.active;
export const activeSession = () => state.sessions.find((s) => s.id === state.active);
export const sessionData = (id: string) => data.get(id);
export const setActiveSession = (id: string) => setState("active", id);

const [softWrap, setSoftWrap] = createSignal(false);
export { softWrap, setSoftWrap };

function bump(id: string) {
  setState("sessions", (s) => s.id === id, "version", (v) => v + 1);
}

let pending = new Set<string>();
let scheduled = false;
/** Coalesce re-renders of busy consoles to one per frame. */
function scheduleBump(id: string) {
  pending.add(id);
  if (scheduled) return;
  scheduled = true;
  const flush = () => {
    scheduled = false;
    const ids = pending;
    pending = new Set();
    batch(() => ids.forEach(bump));
  };
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(flush);
  else setTimeout(flush, 16);
}

function trim(d: SessionData) {
  if (d.lines.length > MAX_LINES) d.lines.splice(0, d.lines.length - MAX_LINES);
}

/** A message from the UI itself (e.g. a failed start). */
function writeError(id: string, text: string) {
  const d = data.get(id);
  if (!d) return;
  d.lines.push({ segments: [{ text, style: { fg: "var(--danger)" } }] });
  trim(d);
  scheduleBump(id);
}

function onEvent(id: string, runId: string, e: RunEvent) {
  const s = state.sessions.find((x) => x.id === id);
  if (!s || s.runId !== runId) return;
  const d = data.get(id)!;
  switch (e.type) {
    case "started":
      setState("sessions", (x) => x.id === id, "status", "running");
      break;
    case "console":
      applyConsoleDelta(d.lines, e);
      trim(d);
      scheduleBump(id);
      break;
    case "tests":
      if (d.tests) applyTestUpdates(d.tests, e.nodes, e.summary);
      scheduleBump(id);
      break;
    case "exited": {
      if (d.tests) publishTestResults(d.tests.root);
      setState("sessions", (x) => x.id === id, { status: "finished", exitCode: e.code ?? (e.signal ? 128 + e.signal : null), endedAt: Date.now() });
      d.unlisten?.();
      d.exited?.(e.code);
      d.exited = undefined;
      break;
    }
  }
}

/** Build the backend request for a configuration. */
function requestFor(cfg: RunConfiguration, runId: string, extra?: SessionData["extra"]): RunRequest {
  const type = findType(cfg.type);
  const req: RunRequest = { runId, env: extra?.env, interpreterArgs: extra?.interpreterArgs, testRunner: !!type?.testRunner };
  if (type?.buildCommand) {
    const cmd = type.buildCommand(cfg);
    req.command = { ...cmd, cwd: cmd.cwd || cfg.workingDirectory || "", env: { ...(cmd.env ?? {}), ...(cfg.env ?? {}) } };
  } else req.configuration = cfg;
  return req;
}

async function startProcess(id: string, extra?: SessionData["extra"]): Promise<RunHandle | null> {
  const s = state.sessions.find((x) => x.id === id)!;
  const d = data.get(id)!;
  d.unlisten?.();
  const runId = `run-${Date.now().toString(36)}-${++seq}`;
  setState("sessions", (x) => x.id === id, { runId, status: "starting", exitCode: undefined, startedAt: Date.now(), endedAt: undefined });
  const exited = new Promise<number | null>((resolve) => (d.exited = resolve));
  d.unlisten = await runApi.onEvent(runId, (e) => onEvent(id, runId, e));
  try {
    const request = requestFor(s.config, runId, extra);
    const start = findType(s.config.type)?.start;
    const started = await (start ? start(s.config, request) : runApi.start(request));
    return { started, exited };
  } catch (err) {
    d.unlisten?.();
    writeError(id, `Error running '${s.name}': ${String(err)}`);
    setState("sessions", (x) => x.id === id, { status: "failed", endedAt: Date.now() });
    d.exited?.(null);
    return null;
  }
}

function resetSession(id: string, cfg: RunConfiguration, debug: boolean) {
  const old = data.get(id);
  old?.unlisten?.();
  const type = findType(cfg.type);
  data.set(id, {
    lines: [],
    tests: type?.testRunner ? emptyTestTree() : undefined,
    extra: old?.extra,
  });
  setState("sessions", (x) => x.id === id, { config: cfg, name: cfg.name, debug, version: 0 });
}

/** Tab for a new run: reuses the finished, unpinned tab of the same configuration. */
function sessionFor(cfg: RunConfiguration, debug: boolean): string {
  const reusable = state.sessions.find(
    (s) => s.name === cfg.name && s.debug === debug && !s.pinned && s.status !== "running" && s.status !== "starting",
  );
  const id = reusable?.id ?? `tab-${++seq}`;
  if (!reusable) {
    setState(
      "sessions",
      produce((list) =>
        list.push({ id, runId: null, name: cfg.name, config: cfg, debug, status: "starting", startedAt: Date.now(), version: 0, pinned: false }),
      ),
    );
  }
  resetSession(id, cfg, debug);
  setState("active", id);
  return id;
}

async function runBeforeLaunch(cfg: RunConfiguration, depth: number): Promise<boolean> {
  for (const task of cfg.beforeLaunch ?? []) {
    let target: RunConfiguration | undefined;
    if (task.type === "runConfiguration") target = findConfiguration(task.value);
    else if (task.type === "shell")
      target = { name: task.value, type: "shell", options: { mode: "text", scriptText: task.value }, workingDirectory: cfg.workingDirectory };
    if (!target) {
      showBalloon(`Before launch task failed`, `Configuration "${task.value}" not found`);
      return false;
    }
    if (depth > 5) return false;
    const handle = await launch(target, { depth: depth + 1 });
    const code = handle ? await handle.exited : null;
    if (code !== 0) {
      showBalloon(`Before launch task "${target.name}" failed`, code === null ? undefined : `Exit code ${code}`);
      return false;
    }
  }
  return true;
}

interface LaunchOptions {
  debug?: boolean;
  extra?: SessionData["extra"];
  depth?: number;
}

/** Run a configuration in the Run tool window. Returns null when it could not start. */
export async function launch(cfg: RunConfiguration, opts: LaunchOptions = {}): Promise<RunHandle | null> {
  const problem = validateConfiguration(cfg);
  if (problem) {
    showBalloon(`Error running '${cfg.name}'`, problem, 5000, [
      { label: "Edit Configuration…", run: () => openRunConfigurations(cfg.name) },
    ]);
    return null;
  }
  await saveAll().catch(() => undefined);
  if (!(await runBeforeLaunch(cfg, opts.depth ?? 0))) return null;
  const id = sessionFor(cfg, !!opts.debug);
  data.get(id)!.extra = opts.extra;
  // A debug launcher may switch to its own tool window once the session starts.
  showToolWindow("bottom", "run");
  return startProcess(id, opts.extra);
}

export async function runConfiguration(cfg: RunConfiguration | undefined = selectedConfiguration()): Promise<void> {
  if (!cfg) {
    openRunConfigurations();
    return;
  }
  // Like PHPStorm's single-instance default: Run on a running configuration reruns it.
  const running = runningFor(cfg);
  if (running && !running.debug) return rerunSession(running.id);
  await launch(cfg);
}

export async function debugConfiguration(cfg: RunConfiguration | undefined = selectedConfiguration()): Promise<void> {
  if (!cfg) {
    openRunConfigurations();
    return;
  }
  const launcher = debugLauncherFor(cfg);
  if (!launcher) {
    showBalloon(
      `Cannot debug '${cfg.name}'`,
      findType(cfg.type)?.debuggable
        ? "No debugger is available. Enable a debugger plugin such as Xdebug."
        : `${findType(cfg.type)?.name ?? cfg.type} configurations cannot be debugged.`,
    );
    return;
  }
  await launcher.launch(cfg, { run: (extra) => launch(cfg, { debug: true, extra }) });
}

export const isRunning = (s: RunSession | undefined) => !!s && (s.status === "running" || s.status === "starting");

export const runningSessions = () => state.sessions.filter(isRunning);

export const runningFor = (cfg: RunConfiguration | undefined) =>
  cfg ? state.sessions.find((s) => s.name === cfg.name && isRunning(s)) : undefined;

export async function stopSession(id: string | undefined = state.active ?? undefined): Promise<void> {
  const s = state.sessions.find((x) => x.id === id) ?? runningSessions()[0];
  if (!s?.runId || !isRunning(s)) return;
  try {
    await runApi.stop(s.runId);
  } catch (err) {
    showBalloon(`Cannot stop '${s.name}'`, String(err));
  }
}

export async function rerunSession(id: string | undefined = state.active ?? undefined): Promise<void> {
  const s = state.sessions.find((x) => x.id === id);
  if (!s) {
    await runConfiguration();
    return;
  }
  if (isRunning(s) && s.runId) {
    await runApi.stop(s.runId).catch(() => undefined);
    await new Promise<void>((r) => {
      const d = data.get(s.id);
      const prev = d?.exited;
      if (!d || !prev) return r();
      d.exited = (c) => (prev(c), r());
    });
  }
  // Pick up edits made to the configuration since the last run.
  const cfg = findConfiguration(s.name) ?? s.config;
  if (validateConfiguration(cfg)) return void launch(cfg);
  await saveAll().catch(() => undefined);
  const extra = data.get(s.id)?.extra;
  resetSession(s.id, cfg, s.debug);
  data.get(s.id)!.extra = extra;
  setState("active", s.id);
  await startProcess(s.id, extra);
}

/** Rerun only the failed tests of a test run. */
export async function rerunFailedTests(id: string | undefined = state.active ?? undefined): Promise<void> {
  const s = state.sessions.find((x) => x.id === id);
  const tree = s && data.get(s.id)?.tests;
  if (!s || !tree) return;
  const failed: string[] = [];
  const walk = (n: TestNode) => {
    if (n.kind === "test" && n.status === "failed") failed.push(n.location?.member ?? n.name);
    n.children.forEach(walk);
  };
  walk(tree.root);
  if (!failed.length) return;
  const filter = `/::(${failed.map((f) => f.replace(/[\\^$.*+?()[\]{}|/]/g, "\\$&")).join("|")})( .*)?$/`;
  const options = { ...(s.config.options ?? {}) };
  options.testRunnerOptions = `${options.testRunnerOptions ?? ""} --filter '${filter}'`.trim();
  await launch({ ...s.config, options });
}

export function clearConsole(id: string | undefined = state.active ?? undefined): void {
  const d = id ? data.get(id) : undefined;
  if (!d) return;
  d.lines.length = 0;
  bump(id!);
}

export function togglePin(id: string): void {
  setState("sessions", (s) => s.id === id, "pinned", (p) => !p);
}

export async function closeSession(id: string): Promise<void> {
  const s = state.sessions.find((x) => x.id === id);
  if (!s) return;
  if (s.runId) await runApi.dispose(s.runId).catch(() => undefined);
  data.get(id)?.unlisten?.();
  data.delete(id);
  const idx = state.sessions.findIndex((x) => x.id === id);
  batch(() => {
    setState("sessions", (list) => list.filter((x) => x.id !== id));
    if (state.active === id) setState("active", state.sessions[Math.min(idx, state.sessions.length - 1)]?.id ?? null);
  });
}

/** Send input typed into the console to the process. */
export async function sendInput(id: string, text: string): Promise<void> {
  const s = state.sessions.find((x) => x.id === id);
  if (!s?.runId || !isRunning(s)) return;
  // The backend echoes the input in the console.
  await runApi.write(s.runId, text).catch((err) => showBalloon("Cannot send input", String(err)));
}
