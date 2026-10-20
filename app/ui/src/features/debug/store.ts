/**
 * Debugger state for the UI: listening toggle, sessions, the suspended context (frames,
 * variables, watches), console output and breakpoints. Fed by `debug://*` events.
 */
import { batch, createMemo, createRoot, createSignal } from "solid-js";
import { showBalloon } from "@/keymap/dispatch";
import { showToolWindow } from "@/state/layout";
import { EditorView } from "@codemirror/view";
import { openFile } from "@/state/editor";
import { viewFor } from "@/features/lsp/navigation";
import {
  debugApi,
  type Breakpoint,
  type BreakpointsSnapshot,
  type DebugEvent,
  type Frame,
  type Property,
  type SessionInfo,
  type StepKind,
} from "./api";

export interface ConsoleLine {
  sessionId: string;
  stream: "stdout" | "stderr" | "system";
  text: string;
}

export type WatchResult = { value: Property } | { error: string } | { pending: true };

const [listening, setListening] = createSignal(false);
const [port, setPort] = createSignal(9003);
const [sessions, setSessions] = createSignal<SessionInfo[]>([]);
const [activeId, setActiveId] = createSignal<string | null>(null);
const [frames, setFrames] = createSignal<Frame[]>([]);
const [frameIndex, setFrameIndex] = createSignal(0);
const [variables, setVariables] = createSignal<Property[]>([]);
const [variablesLoading, setVariablesLoading] = createSignal(false);
const [watches, setWatchList] = createSignal<string[]>([]);
const [watchResults, setWatchResults] = createSignal<Record<string, WatchResult>>({});
const [consoleLines, setConsoleLines] = createSignal<ConsoleLine[]>([]);
const [breakpointState, setBreakpointState] = createSignal<BreakpointsSnapshot>({ breakpoints: [], muted: false });
const [incoming, setIncoming] = createSignal<SessionInfo | null>(null);
/** Bumped whenever variables were reloaded, so expanded tree nodes refetch. */
const [contextVersion, setContextVersion] = createSignal(0);

export {
  listening,
  port,
  sessions,
  activeId,
  frames,
  frameIndex,
  variables,
  variablesLoading,
  watches,
  watchResults,
  consoleLines,
  incoming,
  contextVersion,
};

export const breakpoints = () => breakpointState().breakpoints;
export const breakpointsMuted = () => breakpointState().muted;

const memo = createRoot(() => {
  const activeSession = createMemo(() => sessions().find((s) => s.id === activeId()) ?? null);
  const paused = createMemo(() => {
    const s = activeSession();
    return s && s.status === "paused" ? s : null;
  });
  return { activeSession, paused };
});

export const activeSession = memo.activeSession;
/** The active session if it is suspended. */
export const pausedSession = memo.paused;
export const selectedFrame = (): Frame | undefined => frames()[frameIndex()];

/** Execution point of the active session (top frame). */
export function executionPoint(): { file: string; line: number } | null {
  const loc = pausedSession()?.location;
  return loc?.file ? { file: loc.file, line: loc.line } : null;
}

export function selectSession(id: string): void {
  setActiveId(id);
  const s = sessions().find((x) => x.id === id);
  if (s?.status === "paused") void loadSuspendContext(s);
  else clearContext();
}

function clearContext(): void {
  batch(() => {
    setFrames([]);
    setFrameIndex(0);
    setVariables([]);
    setWatchResults({});
  });
}

function log(sessionId: string, stream: ConsoleLine["stream"], text: string): void {
  setConsoleLines((lines) => [...lines.slice(-2000), { sessionId, stream, text }]);
}

async function loadVariables(sessionId: string, depth: number): Promise<void> {
  setVariablesLoading(true);
  try {
    const vars = await debugApi.variables(sessionId, depth, 0);
    batch(() => {
      setVariables(vars);
      setContextVersion((v) => v + 1);
    });
  } catch (e) {
    setVariables([]);
    console.warn("variables", e);
  } finally {
    setVariablesLoading(false);
  }
  await evaluateWatches();
}

async function loadSuspendContext(session: SessionInfo): Promise<void> {
  try {
    const stack = await debugApi.stack(session.id);
    batch(() => {
      setFrames(stack);
      setFrameIndex(0);
    });
  } catch (e) {
    setFrames([]);
    console.warn("stack", e);
  }
  await loadVariables(session.id, 0);
}

/** Select a frame: shows its variables and opens its source line. */
export async function selectFrame(index: number): Promise<void> {
  const s = pausedSession();
  const f = frames()[index];
  if (!s || !f) return;
  setFrameIndex(index);
  if (f.file) void showLine(f.file, f.line);
  await loadVariables(s.id, f.level);
}

export async function evaluateWatches(): Promise<void> {
  const s = pausedSession();
  if (!s) {
    setWatchResults({});
    return;
  }
  const depth = selectedFrame()?.level ?? 0;
  const list = watches();
  const results: Record<string, WatchResult> = {};
  await Promise.all(
    list.map(async (w) => {
      try {
        results[w] = { value: await debugApi.eval(s.id, w, depth) };
      } catch (e) {
        results[w] = { error: String(e).replace(/^Error: /, "") };
      }
    }),
  );
  setWatchResults(results);
}

export function addWatch(expression: string): void {
  const e = expression.trim();
  if (!e || watches().includes(e)) return;
  setWatchList((w) => [...w, e]);
  void debugApi.setWatches(watches()).catch(() => {});
  void evaluateWatches();
}

export function removeWatch(expression: string): void {
  setWatchList((w) => w.filter((x) => x !== expression));
  void debugApi.setWatches(watches()).catch(() => {});
}

/**
 * Open `path` and scroll `line` (1-based) to the middle without moving the caret, like
 * PHPStorm does for the execution point.
 */
export async function showLine(path: string, line: number): Promise<void> {
  try {
    await openFile(path);
  } catch {
    showBalloon("Source not found", `${path} is not available in the project.`);
    return;
  }
  for (let i = 0; i < 60; i++) {
    const view = viewFor(path);
    if (view && view.dom.isConnected && view.dom.offsetParent !== null) {
      const l = view.state.doc.line(Math.max(1, Math.min(line, view.state.doc.lines)));
      view.dispatch({ effects: EditorView.scrollIntoView(l.from, { y: "center" }) });
      return;
    }
    await new Promise((r) => requestAnimationFrame(() => r(null)));
  }
}

// ---- events ---------------------------------------------------------------------------

function upsert(session: SessionInfo): SessionInfo | undefined {
  const prev = sessions().find((s) => s.id === session.id);
  setSessions((list) => (prev ? list.map((s) => (s.id === session.id ? session : s)) : [...list, session]));
  return prev;
}

function onSession(session: SessionInfo): void {
  const prev = upsert(session);
  if (!prev) log(session.id, "system", `Connected to ${session.engine || "Xdebug"} (${session.serverName}, ${session.file ?? session.fileUri})\n`);
  if (!activeId() || !sessions().some((s) => s.id === activeId())) setActiveId(session.id);
  if (session.status === "paused") {
    if (activeId() !== session.id && activeSession()?.status !== "paused") setActiveId(session.id);
    if (activeId() !== session.id) return;
    showToolWindow("bottom", "debug");
    const loc = session.location;
    if (loc?.file) void showLine(loc.file, loc.line);
    if (session.exception) log(session.id, "stderr", `${session.exception.name}: ${session.exception.message ?? ""}\n`);
    void loadSuspendContext(session);
  } else if (session.id === activeId()) {
    clearContext();
  }
}

function onEnded(sessionId: string): void {
  log(sessionId, "system", "Disconnected\n");
  setSessions((list) => list.filter((s) => s.id !== sessionId));
  if (incoming()?.id === sessionId) setIncoming(null);
  if (activeId() === sessionId) {
    const next = sessions().find((s) => s.status === "paused") ?? sessions()[0];
    setActiveId(next?.id ?? null);
    if (next?.status === "paused") void loadSuspendContext(next);
    else clearContext();
  }
}

export function handleDebugEvent(e: DebugEvent): void {
  switch (e.type) {
    case "listening":
      setListening(e.listening);
      setPort(e.port);
      if (e.error) showBalloon("Cannot start listening for debug connections", e.error, 6000);
      break;
    case "session":
      onSession(e.session);
      break;
    case "incoming":
      upsert(e.session);
      setIncoming(e.session);
      break;
    case "ended":
      onEnded(e.sessionId);
      break;
    case "output":
      log(e.sessionId, e.stream === "stderr" ? "stderr" : "stdout", e.text);
      break;
    case "breakpoints":
      setBreakpointState(e.snapshot);
      break;
    case "error":
      showBalloon("Debugger error", e.message, 5000);
      break;
  }
}

let started = false;

/** Load the initial state and subscribe to debugger events (once). */
export async function startDebugStore(): Promise<void> {
  if (started) return;
  started = true;
  await debugApi.onEvent(handleDebugEvent);
  await reloadDebugState();
}

/** Re-read status, sessions and breakpoints (e.g. after opening another project). */
export async function reloadDebugState(): Promise<void> {
  try {
    const [status, bps, w] = await Promise.all([debugApi.status(), debugApi.breakpoints(), debugApi.watches()]);
    batch(() => {
      setListening(status.listening);
      setPort(status.port);
      setSessions(status.sessions);
      setBreakpointState(bps);
      setWatchList(w);
    });
    const paused = status.sessions.find((s) => s.status === "paused");
    const awaiting = status.sessions.find((s) => s.status === "awaitingAcceptance");
    if (awaiting) setIncoming(awaiting);
    if (paused) {
      setActiveId(paused.id);
      await loadSuspendContext(paused);
    } else if (status.sessions[0]) setActiveId(status.sessions[0].id);
  } catch (e) {
    console.warn("debugger state unavailable", e);
  }
}

// ---- actions --------------------------------------------------------------------------

export async function toggleListening(): Promise<void> {
  try {
    if (listening()) await debugApi.listenStop();
    else await debugApi.listenStart();
  } catch (e) {
    showBalloon("Cannot start listening for debug connections", String(e), 6000);
  }
}

export async function step(kind: StepKind): Promise<void> {
  const s = pausedSession();
  if (!s) return;
  try {
    await debugApi.step(s.id, kind);
  } catch (e) {
    showBalloon("Debugger", String(e));
  }
}

export async function pause(): Promise<void> {
  const s = activeSession();
  if (s?.status === "running") await debugApi.pause(s.id).catch((e) => showBalloon("Pause is not supported by this engine", String(e)));
}

export async function stop(): Promise<boolean> {
  const s = activeSession();
  if (!s) return false;
  await debugApi.stop(s.id).catch((e) => console.warn("stop", e));
  return true;
}

export async function runToCursor(file: string, line: number): Promise<void> {
  const s = pausedSession();
  if (!s) return;
  await debugApi.runToCursor(s.id, file, line).catch((e) => showBalloon("Run to Cursor", String(e)));
}

export async function toggleLineBreakpoint(file: string, line: number): Promise<void> {
  await debugApi.toggleBreakpoint(file, line).catch((e) => showBalloon("Cannot toggle breakpoint", String(e)));
}

export async function updateBreakpoint(bp: Breakpoint): Promise<void> {
  await debugApi.updateBreakpoint(bp).catch((e) => showBalloon("Cannot update breakpoint", String(e)));
}

export async function removeBreakpoint(id: number): Promise<void> {
  await debugApi.removeBreakpoint(id).catch((e) => showBalloon("Cannot remove breakpoint", String(e)));
}

export async function toggleMuted(): Promise<void> {
  await debugApi.mute(!breakpointsMuted()).catch((e) => showBalloon("Cannot mute breakpoints", String(e)));
}

export function clearConsole(): void {
  const id = activeId();
  setConsoleLines((lines) => lines.filter((l) => l.sessionId !== id));
}

export { setIncoming };
