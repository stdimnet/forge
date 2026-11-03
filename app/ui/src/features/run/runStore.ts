/**
 * Runs shown in the Run tool window: one tab per run, its console and, for test runners, the
 * test tree (both computed by the backend and streamed as `console` / `tests` run events),
 * and Run / Debug / Stop / Rerun.
 */
import { batch, createSignal, type Component } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { runApi, type ConsoleLine, type RunConfiguration, type RunEvent, type RunRequest } from "@/ipc/run";
import { runToolsApi, type LaunchPlan, type PlannedTask } from "@/ipc/runTools";
import { balloonsFor } from "@/keymap";
import { onFileSaved, saveAll } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { getSetting } from "@/state/settings";
import { testFrameworksApi, type FailedTest, type StoredRun } from "@/ipc/testFrameworks";
import { testingApi, type CoverageSuite } from "@/ipc/testing";
import { applyConsoleDelta, applyTestUpdates, emptyTestTree, type TestNode, type TestTreeState } from "./tests";
import { COMPOUND, compoundMembers, debugLauncherFor, findType, validateConfiguration, type RunHandle } from "./types";
import { allConfigurations, findConfiguration, selectedConfiguration } from "./configStore";
import { openRunConfigurations } from "./dialogState";
import { fileOf, publishTestResults } from "./testResults";

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
  /** Toggle Auto-Test: rerun after files are saved. */
  autoTest?: boolean;
  /** The shown result panel (`registerRunPanel`); the console when unset. */
  panel?: string;
  /** A run of another machine shown here (Code With Me: the host's run id in a guest's window). */
  shared?: number;
}

interface SessionData {
  lines: ConsoleLine[];
  tests?: TestTreeState;
  exited?: (code: number | null) => void;
  unlisten?: () => void;
  extra?: Pick<RunRequest, "env" | "interpreterArgs" | "coverage" | "debugPort" | "profile" | "nativeTool" | "profiler">;
  /** Data of result panels (`registerRunPanel`), by panel id. */
  panels?: Record<string, unknown>;
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

type CoverageListener = (suite: CoverageSuite) => void;
const coverageListeners: CoverageListener[] = [];

/** Coverage suites of finished Run with Coverage runs (the Coverage feature listens). */
export function onCoverageSuite(l: CoverageListener): void {
  coverageListeners.push(l);
}

type ProfileListener = (profile: import("@/ipc/pythonTools").ProfileInfo) => void;
const profileListeners: ProfileListener[] = [];

/** Snapshots of finished Profile runs (the Profiler tool window listens). */
export function onProfileSnapshot(l: ProfileListener): void {
  profileListeners.push(l);
}

/**
 * A result tab of Run sessions next to the console (CForge's Sanitizers and Valgrind): it
 * takes the run events it `accepts` and shows their data.
 */
export interface RunPanelDef {
  id: string;
  title: string;
  icon?: Component<{ size?: number }>;
  /** The panel's new data for a run event (given the current data), or undefined. */
  accepts: (e: RunEvent, current: unknown) => unknown;
  /** The number next to the title. */
  count?: (data: unknown) => number;
  /** Show the panel when data arrives. */
  reveal?: (data: unknown) => boolean;
  component: Component<{ data: unknown; session: RunSession }>;
}

const [panelDefs, setPanelDefs] = createSignal<RunPanelDef[]>([]);
export { panelDefs as runPanels };

export function registerRunPanel(def: RunPanelDef): void {
  setPanelDefs((prev) => [...prev.filter((p) => p.id !== def.id), def]);
}

/** The panels with data of a session. */
export const sessionPanels = (id: string): Record<string, unknown> => data.get(id)?.panels ?? {};

export function setSessionPanel(id: string, panel: string | undefined): void {
  setState("sessions", (s) => s.id === id, "panel", panel);
}

type RunEventListener = (session: RunSession, e: RunEvent) => void;
const eventListeners: RunEventListener[] = [];

/** Run events the run feature does not handle itself (CForge's perf profiles). */
export function onRunEvent(l: RunEventListener): void {
  eventListeners.push(l);
}

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
    case "coverage":
      if (e.suite) coverageListeners.forEach((l) => l(e.suite!));
      else if (e.error) writeError(id, `Coverage: ${e.error}`);
      break;
    case "profile":
      if (e.profile) profileListeners.forEach((l) => l(e.profile!));
      else if (e.error) writeError(id, `Profiler: ${e.error}`);
      break;
    case "sanitizer":
    case "valgrind":
    case "perf":
      for (const p of panelDefs()) {
        const next = p.accepts(e, d.panels?.[p.id]);
        if (next === undefined) continue;
        d.panels = { ...d.panels, [p.id]: next };
        if (p.reveal?.(next)) setSessionPanel(id, p.id);
        scheduleBump(id);
      }
      if ("error" in e && e.error) writeError(id, `${e.type === "perf" ? "perf" : "Valgrind"}: ${e.error}`);
      eventListeners.forEach((l) => l(s, e));
      break;
    case "profiler":
      if (e.error) writeError(id, `Profiler: ${e.error}`);
      eventListeners.forEach((l) => l(s, e));
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

// ---- Runs of another machine (Code With Me) ----------------------------------------------

/**
 * Where runs happen when this window shows another machine's runs (a Code With Me guest:
 * the host runs its own configurations; the guest only names them).
 */
export interface RemoteRuns {
  /** Run (or rerun) a configuration by name. */
  start(cfg: RunConfiguration): Promise<void>;
  stop(run: number): Promise<void>;
  input(run: number, text: string): Promise<void>;
  /** The run's events so far; live events follow on `run://<runId>` of {@link showSharedRun}. */
  attach(run: number): Promise<RunEvent[]>;
  /** Its tab closed. */
  detach(run: number): void;
  /** Why Debug and Run with Coverage are not available. */
  unavailable: string;
}

let remote: RemoteRuns | null = null;

/** Route Run, Stop, Rerun and console input to another machine (`null`: this machine). */
export function setRemoteRuns(r: RemoteRuns | null): void {
  remote = r;
}

export const remoteRuns = () => remote;

/** A run of another machine, as it is listed there. */
export interface SharedRunInfo {
  id: number;
  title: string;
  kind?: string;
  running: boolean;
  exitCode?: number;
}

/**
 * Show a run of another machine in a tab (reusing the finished, unpinned tab of the same
 * name, as for local runs): what happened so far, then its live events on `run://<runId>`.
 */
export async function showSharedRun(run: SharedRunInfo, runId: string, reveal = false): Promise<void> {
  if (!remote || state.sessions.some((s) => s.shared === run.id)) return;
  const reusable = state.sessions.find((s) => s.shared !== undefined && s.name === run.title && !s.pinned && !isRunning(s));
  const id = reusable?.id ?? `tab-${++seq}`;
  const config: RunConfiguration = { name: run.title, type: run.kind ?? "shared" };
  const session = { runId, shared: run.id, status: (run.running ? "running" : "finished") as RunStatus, exitCode: run.exitCode, startedAt: Date.now() };
  if (reusable) setState("sessions", (x) => x.id === id, { ...session, endedAt: undefined });
  else setState("sessions", produce((list) => list.push({ id, name: run.title, config, debug: false, version: 0, pinned: false, ...session })));
  data.get(id)?.unlisten?.();
  data.set(id, { lines: [], tests: findType(config.type)?.testRunner ? emptyTestTree() : undefined });
  bump(id);
  if (reveal) {
    setState("active", id);
    showToolWindow("run");
  } else if (!state.active) setState("active", id);
  // Live events wait until what happened before is shown.
  let queued: RunEvent[] | null = [];
  const d = data.get(id)!;
  d.unlisten = await runApi.onEvent(runId, (e) => (queued ? queued.push(e) : onEvent(id, runId, e)));
  const before = await remote.attach(run.id).catch(() => [] as RunEvent[]);
  for (const e of [...before, ...queued]) {
    if (e.type === "tests" && !d.tests) d.tests = emptyTestTree();
    onEvent(id, runId, e);
  }
  queued = null;
}

/** The other machine is gone (the session ended): its runs no longer report anything. */
export function endSharedRuns(): void {
  for (const s of state.sessions.filter((x) => x.shared !== undefined)) {
    data.get(s.id)?.unlisten?.();
    if (isRunning(s)) setState("sessions", (x) => x.id === s.id, { status: "finished", endedAt: Date.now() });
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
    coverage: extra?.coverage || undefined,
    debugPort: extra?.debugPort,
    profile: extra?.profile || undefined,
    nativeTool: extra?.nativeTool,
    profiler: extra?.profiler,
    runOn: cfg.runOn?.kind && cfg.runOn.kind !== "local" ? cfg.runOn : undefined,
  };
  if (type?.buildCommand) {
    const cmd = type.buildCommand(cfg);
    req.command = { ...cmd, cwd: cmd.cwd || cfg.workingDirectory || "", env: { ...(cmd.env ?? {}), ...(cfg.env ?? {}) } };
    req.name = cfg.name;
    req.typeId = cfg.type;
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
  setState("sessions", (x) => x.id === id, { config: cfg, name: cfg.name, debug, version: 0, panel: undefined });
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
  if (remote) {
    if (opts.debug || opts.extra) {
      showBalloon(`Cannot run '${cfg.name}'`, remote.unavailable);
      return null;
    }
    await remote.start(cfg).catch((err) => showBalloon(`Error running '${cfg.name}'`, String((err as Error)?.message ?? err)));
    return null;
  }
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

/** Run with Coverage: the configuration's tests write a coverage report the IDE shows. */
export async function runWithCoverage(cfg: RunConfiguration | undefined = selectedConfiguration()): Promise<void> {
  if (!cfg) {
    openRunConfigurations();
    return;
  }
  if (!findType(cfg.type)?.coverage) {
    showBalloon(`Cannot run '${cfg.name}' with coverage`, `${findType(cfg.type)?.name ?? cfg.type} configurations cannot measure coverage.`);
    return;
  }
  await launch(cfg, { extra: { coverage: true } });
}

export async function debugConfiguration(cfg: RunConfiguration | undefined = selectedConfiguration()): Promise<void> {
  if (remote) return void showBalloon(`Cannot debug '${cfg?.name ?? "the configuration"}'`, remote.unavailable);
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
  await launcher.launch(cfg, {
    run: (extra) => launch(cfg, { debug: true, extra }),
    target: () => (cfg.runOn && cfg.runOn.kind !== "local" ? testingApi.runTargetInfo(cfg).catch(() => null) : Promise.resolve(null)),
  });
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
    if (s.shared !== undefined) {
      if (remote) await remote.stop(s.shared);
      return;
    }
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
  // The other machine reruns its configuration (a running one restarts there).
  if (s.shared !== undefined) {
    if (remote) await launch(findConfiguration(s.name) ?? s.config);
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

/** The failed tests of a run's tree. */
export function failedTests(root: TestNode): FailedTest[] {
  const out: FailedTest[] = [];
  const walk = (n: TestNode) => {
    if (n.kind === "test" && n.status === "failed") out.push({ name: n.name, member: n.location?.member, path: fileOf(n) });
    n.children.forEach(walk);
  };
  walk(root);
  return out;
}

/**
 * Rerun only the failed tests of a test run, with the configuration Rust derives for the
 * framework (PHPUnit / Pest `--filter`, Codeception `-g failed`, Behat `--rerun`, PHPSpec
 * the failed spec).
 */
export async function rerunFailedTests(id: string | undefined = state.active ?? undefined): Promise<void> {
  const s = state.sessions.find((x) => x.id === id);
  const tree = s && data.get(s.id)?.tests;
  if (!s || !tree) return;
  const failed = failedTests(tree.root);
  if (!failed.length) return;
  const base = findConfiguration(s.name) ?? s.config;
  const cfg = await testFrameworksApi.rerunFailed(base, failed).catch((err) => {
    showBalloon("Cannot rerun the failed tests", String(err));
    return null;
  });
  if (cfg) await launch(cfg);
}

/** Open a run of the Test History in a Run tab; Rerun runs its configuration again. */
export function openHistoryRun(run: StoredRun): void {
  const id = sessionFor(run.configuration, false);
  const d = data.get(id)!;
  const tree = emptyTestTree();
  applyTestUpdates(tree, run.nodes, run.summary);
  d.tests = tree;
  d.lines = [
    {
      segments: [{ text: `Test results of ${new Date(run.entry.at).toLocaleString()} from the Test History`, style: { fg: "var(--fg-muted)" } }],
    },
  ];
  setState("sessions", (x) => x.id === id, {
    runId: null,
    status: "finished",
    startedAt: run.entry.at,
    endedAt: run.entry.at + (tree.root.duration ?? 0),
    exitCode: run.entry.counts.failed ? 1 : 0,
  });
  bump(id);
  showToolWindow("run");
}

// ---- Auto-test -----------------------------------------------------------------------

const autoTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** Toggle Auto-Test: the session reruns `run.autoTest.delay` seconds after files are saved. */
export function toggleAutoTest(id: string | undefined = state.active ?? undefined): void {
  const s = state.sessions.find((x) => x.id === id);
  if (!s) return;
  setState("sessions", (x) => x.id === s.id, "autoTest", (v) => !v);
  if (!state.sessions.find((x) => x.id === s.id)?.autoTest) {
    clearTimeout(autoTimers.get(s.id));
    autoTimers.delete(s.id);
  }
}

function scheduleAutoTest(id: string): void {
  clearTimeout(autoTimers.get(id));
  const delay = Math.max(0, Number(getSetting<number>("run.autoTest.delay", 3)) || 0) * 1000;
  autoTimers.set(
    id,
    setTimeout(() => {
      autoTimers.delete(id);
      const s = state.sessions.find((x) => x.id === id);
      if (!s?.autoTest) return;
      // A run in progress finishes first; the saved changes rerun afterwards.
      if (isRunning(s)) return scheduleAutoTest(id);
      void rerunSession(id);
    }, delay),
  );
}

onFileSaved(() => {
  for (const s of state.sessions) if (s.autoTest) scheduleAutoTest(s.id);
});

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
  if (s.shared !== undefined) remote?.detach(s.shared);
  else if (s.runId) await runApi.dispose(s.runId).catch(() => undefined);
  data.get(id)?.unlisten?.();
  data.delete(id);
  clearTimeout(autoTimers.get(id));
  autoTimers.delete(id);
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
  const sent = s.shared !== undefined ? remote?.input(s.shared, text) : runApi.write(s.runId, text);
  await sent?.catch((err) => showBalloon("Cannot send input", String(err)));
}
