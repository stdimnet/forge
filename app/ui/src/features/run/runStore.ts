/**
 * Runs shown in the Run tool window: one tab per run, its console and, for test runners, the
 * test tree (both computed by the backend and streamed as `console` / `tests` run events),
 * and Run / Debug / Stop / Rerun.
 */
import { batch, createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { runApi, type ConsoleLine, type RunConfiguration, type RunEvent, type RunRequest } from "@/ipc/run";
import { runToolsApi, type LaunchPlan, type PlannedTask } from "@/ipc/runTools";
import { balloonsFor } from "@/keymap";
import { saveAll } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { applyConsoleDelta, applyTestUpdates, emptyTestTree, type TestNode, type TestTreeState } from "./tests";
import { COMPOUND, compoundMembers, debugLauncherFor, findType, validateConfiguration, type RunHandle } from "./types";
import { allConfigurations, findConfiguration, selectedConfiguration } from "./configStore";
import { openRunConfigurations } from "./dialogState";
import { publishTestResults } from "./testResults";

const showBalloon = balloonsFor("Run");

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
  /** Output without a process (Analyze Stack Trace); Rerun runs `SessionData.rerun`. */
  staticOutput?: boolean;
}

interface SessionData {
  lines: ConsoleLine[];
  tests?: TestTreeState;
  exited?: (code: number | null) => void;
  unlisten?: () => void;
  extra?: Pick<RunRequest, "env" | "interpreterArgs">;
  /** Rerun of a tab without a process. */
  rerun?: () => void;
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
  const req: RunRequest = {
    runId,
    env: extra?.env,
    interpreterArgs: extra?.interpreterArgs,
    testRunner: !!type?.testRunner,
    outputFilters: type?.outputFilters?.(cfg),
  };
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

// ---- Launch plans: compound configurations and Before launch tasks ---------------------

/**
 * Runs a Before launch task kind the run feature does not know itself (`externalTool`);
 * resolves `true` when the task succeeded.
 */
export type BeforeLaunchHandler = (task: PlannedTask, owner: RunConfiguration) => Promise<boolean>;
const taskHandlers = new Map<string, BeforeLaunchHandler>();

/** Handle a Before launch task kind, e.g. "Run External tool" from the External Tools feature. */
export function registerBeforeLaunchHandler(kind: string, handler: BeforeLaunchHandler): void {
  taskHandlers.set(kind, handler);
}

/** Open a URL in the browser ("Launch Web Browser"). */
export function openInBrowser(url: string): void {
  window.open(url, "_blank", "noopener");
}

/** What starting `cfg` runs, computed in Rust (`forge-runtools::launch`). */
async function planFor(cfg: RunConfiguration): Promise<LaunchPlan | null> {
  if (cfg.type !== COMPOUND && !cfg.beforeLaunch?.length) {
    return { runs: [{ configuration: cfg.name, tasks: [] }], compound: false, allowMultipleInstances: false };
  }
  const known = allConfigurations();
  const list = known.some((c) => c.name === cfg.name) ? known.map((c) => (c.name === cfg.name ? cfg : c)) : [...known, cfg];
  try {
    return await runToolsApi.plan(list, cfg.name);
  } catch (err) {
    showBalloon(`Error running '${cfg.name}'`, String(err), 5000, [
      { label: "Edit Configuration…", run: () => openRunConfigurations(cfg.name) },
    ]);
    return null;
  }
}

const succeeded = async (handle: RunHandle | null) => !!handle && (await handle.exited) === 0;

/** Run one Before launch task and wait for it; `false` aborts the launch. */
async function runTask(task: PlannedTask, owner: RunConfiguration): Promise<boolean> {
  let ok: boolean;
  if (task.configuration) {
    const target = findConfiguration(task.configuration);
    ok = !!target && (await succeeded(await startConfiguration(target)));
  } else if (task.run) {
    ok = await succeeded(await startConfiguration(task.run));
  } else if (task.type === "browser") {
    openInBrowser(task.value);
    ok = true;
  } else {
    const handler = taskHandlers.get(task.type);
    if (!handler) {
      showBalloon("Before launch task failed", `${task.label}: this task is not available`);
      return false;
    }
    ok = await handler(task, owner).catch(() => false);
  }
  if (!ok) showBalloon(`Before launch task failed`, `${task.label} did not finish successfully, '${owner.name}' was not started.`);
  return ok;
}

interface LaunchOptions {
  debug?: boolean;
  extra?: SessionData["extra"];
  /** Do not bring up the Run tool window (External Tools without "Open console"). */
  background?: boolean;
}

/** Start one (non-compound) configuration in its Run tab, without Before launch tasks. */
async function startConfiguration(cfg: RunConfiguration, opts: LaunchOptions = {}): Promise<RunHandle | null> {
  const problem = validateConfiguration(cfg);
  if (problem) {
    showBalloon(`Error running '${cfg.name}'`, problem, 5000, [
      { label: "Edit Configuration…", run: () => openRunConfigurations(cfg.name) },
    ]);
    return null;
  }
  const id = sessionFor(cfg, !!opts.debug);
  data.get(id)!.extra = opts.extra;
  // A debug launcher may switch to its own tool window once the session starts.
  if (!opts.background) showToolWindow("run");
  const handle = await startProcess(id, opts.extra);
  if (handle) findType(cfg.type)?.afterStart?.(cfg, handle, !!opts.debug);
  return handle;
}

/** Stop a running session and wait until it exited. */
async function stopAndWait(s: RunSession): Promise<void> {
  if (!s.runId || !isRunning(s)) return;
  const d = data.get(s.id);
  const done = new Promise<void>((r) => {
    const prev = d?.exited;
    if (!d || !prev) return r();
    d.exited = (c) => (prev(c), r());
  });
  await runApi.stop(s.runId).catch(() => undefined);
  await done;
}

/**
 * Run a configuration in the Run tool window: its Before launch tasks one after another
 * (aborting on the first failure), then the configuration; a compound starts each member in
 * its own tab. Returns the (first) started run, null when nothing started.
 */
export async function launch(cfg: RunConfiguration, opts: LaunchOptions = {}): Promise<RunHandle | null> {
  const problem = validateConfiguration(cfg);
  if (problem) {
    showBalloon(`Error running '${cfg.name}'`, problem, 5000, [
      { label: "Edit Configuration…", run: () => openRunConfigurations(cfg.name) },
    ]);
    return null;
  }
  await saveAll().catch(() => undefined);
  const plan = await planFor(cfg);
  if (!plan) return null;
  let first: RunHandle | null = null;
  for (const run of plan.runs) {
    const target = run.configuration === cfg.name ? cfg : findConfiguration(run.configuration);
    if (!target) continue;
    // A compound without "Allow multiple instances" restarts members that still run.
    if (plan.compound && !plan.allowMultipleInstances) {
      const running = runningFor(target);
      if (running) await stopAndWait(running);
    }
    let ok = true;
    for (const task of run.tasks) {
      if (!(await runTask(task, target))) {
        ok = false;
        break;
      }
    }
    if (!ok) {
      if (plan.compound) continue;
      return null;
    }
    const handle = await startConfiguration(target, plan.compound ? {} : opts);
    first ??= handle;
  }
  return first;
}

export async function runConfiguration(cfg: RunConfiguration | undefined = selectedConfiguration()): Promise<void> {
  if (!cfg) {
    openRunConfigurations();
    return;
  }
  if (cfg.type === COMPOUND) {
    await launch(cfg);
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
  // A compound debugs each member that can be debugged.
  if (cfg.type === COMPOUND) {
    for (const name of compoundMembers(cfg)) {
      const member = findConfiguration(name);
      if (member) await debugConfiguration(member);
    }
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

/** Names of the configurations a compound starts (nested compounds expanded). */
export function memberNames(cfg: RunConfiguration, seen = new Set<string>()): string[] {
  if (cfg.type !== COMPOUND) return [cfg.name];
  if (seen.has(cfg.name)) return [];
  seen.add(cfg.name);
  return compoundMembers(cfg).flatMap((n) => {
    const member = findConfiguration(n);
    return member ? memberNames(member, seen) : [];
  });
}

/** The running session of a configuration (a compound: of one of its members). */
export const runningFor = (cfg: RunConfiguration | undefined) => {
  if (!cfg) return undefined;
  const names = memberNames(cfg);
  return state.sessions.find((s) => names.includes(s.name) && isRunning(s));
};

/** Stop a configuration; a compound stops all its running members. */
export async function stopConfiguration(cfg: RunConfiguration | undefined = selectedConfiguration()): Promise<void> {
  if (!cfg) return stopSession();
  const names = memberNames(cfg);
  const running = state.sessions.filter((s) => names.includes(s.name) && isRunning(s));
  await Promise.all(running.map((s) => stopSession(s.id)));
}

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
  if (s.staticOutput) {
    data.get(s.id)?.rerun?.();
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

/**
 * A tab of console lines without a process, e.g. an analyzed stack trace: replaces the
 * unpinned tab of the same name. Returns the tab id.
 */
export function openOutputTab(opts: { name: string; type: string; lines: ConsoleLine[]; rerun?: () => void }): string {
  const reusable = state.sessions.find((s) => s.name === opts.name && s.staticOutput && !s.pinned);
  const id = reusable?.id ?? `tab-${++seq}`;
  const now = Date.now();
  if (!reusable) {
    setState(
      "sessions",
      produce((list) =>
        list.push({
          id,
          runId: null,
          name: opts.name,
          config: { name: opts.name, type: opts.type },
          debug: false,
          status: "finished",
          startedAt: now,
          endedAt: now,
          version: 0,
          pinned: false,
          staticOutput: true,
        }),
      ),
    );
  }
  data.set(id, { lines: [...opts.lines], rerun: opts.rerun });
  bump(id);
  setState("active", id);
  showToolWindow("run");
  return id;
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
