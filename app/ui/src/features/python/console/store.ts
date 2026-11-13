/**
 * The Python Console's state: one REPL process in the project's interpreter (IPython when
 * installed), its rendered output, the prompt, the command history and the variables of
 * its namespace. The process and its protocol live in Rust (`python_console_*`,
 * `forge_console.py`).
 */
import { createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import type { ConsoleLine } from "@/ipc/run";
import { pythonToolsApi, type ConsoleEvent, type ConsoleMessage, type ConsoleVariable } from "@/ipc/pythonTools";
import { applyConsoleDelta } from "@/features/run/tests";
import { showToolWindow } from "@/state/layout";

export type ConsoleStatus = "idle" | "starting" | "running" | "exited";

interface State {
  id: string | null;
  status: ConsoleStatus;
  interpreter: string;
  ipython: boolean;
  /** The prompt of the next input (`>>> `, `... `, `In [3]: `, an `input()` prompt). */
  prompt: string;
  /** Lines typed so far of an incomplete statement. */
  pending: string[];
  /** The program waits in `input()`. */
  reading: boolean;
  busy: boolean;
  vars: ConsoleVariable[];
  /** Members of expanded variables, by expression. */
  children: Record<string, ConsoleVariable[]>;
  expanded: Record<string, boolean>;
  showVariables: boolean;
}

const [state, setState] = createStore<State>({
  id: null,
  status: "idle",
  interpreter: "",
  ipython: false,
  prompt: ">>> ",
  pending: [],
  reading: false,
  busy: false,
  vars: [],
  children: {},
  expanded: {},
  showVariables: true,
});
export { state as consoleState };

const lines: ConsoleLine[] = [];
const [version, setVersion] = createSignal(0);
export const consoleLines = () => lines;
export const consoleVersion = version;
const bump = () => setVersion((v) => v + 1);

let history: string[] = [];
let more = "... ";
let primary = ">>> ";
let unlisten: (() => void) | undefined;
let seq = 0;
let ready: Promise<void> | null = null;
let markReady: (() => void) | null = null;

function system(text: string, color = "var(--fg-muted)"): void {
  for (const t of text.split("\n")) lines.push({ segments: [{ text: t, style: { fg: color } }] });
  bump();
}

function onMessage(m: ConsoleMessage): void {
  switch (m.type) {
    case "ready":
      primary = m.prompt;
      more = m.more;
      setState({ ipython: m.ipython, prompt: m.prompt, status: "running", busy: false });
      system(m.banner);
      markReady?.();
      break;
    case "more":
      setState({ prompt: m.prompt, busy: false });
      break;
    case "done":
      primary = m.prompt;
      setState({ prompt: m.prompt, pending: [], busy: false, reading: false });
      break;
    case "vars":
      setState(
        produce((s) => {
          s.vars = m.vars;
          // Members of expanded values are read again.
          s.children = {};
        }),
      );
      for (const expr of Object.keys(state.expanded).filter((e) => state.expanded[e])) void loadChildren(expr);
      break;
    case "children":
      setState("children", m.expr, m.vars);
      break;
    case "input":
      setState({ reading: true, prompt: m.prompt, busy: false });
      break;
  }
}

function onEvent(e: ConsoleEvent): void {
  if (e.type === "console") {
    applyConsoleDelta(lines, e);
    bump();
  } else if (e.type === "message") onMessage(e.message);
  else if (e.type === "exited") {
    setState({ status: "exited", busy: false, reading: false });
    system(`\nProcess finished with exit code ${e.code ?? "?"}`);
    unlisten?.();
    unlisten = undefined;
  }
}

/** Start the console (again). */
export async function startConsole(): Promise<void> {
  if (state.status === "starting" || state.status === "running") return ready ?? undefined;
  const id = `python-console-${++seq}`;
  lines.length = 0;
  setState({ id, status: "starting", prompt: ">>> ", pending: [], vars: [], children: {}, busy: true, reading: false });
  ready = new Promise((r) => (markReady = r));
  unlisten?.();
  unlisten = await pythonToolsApi.onConsole(id, onEvent);
  try {
    const started = await pythonToolsApi.consoleStart(id);
    setState({ interpreter: started.interpreter });
    lines.push({ segments: [{ text: started.commandLine, style: { fg: "var(--fg-muted)" } }] });
    bump();
  } catch (e) {
    setState({ status: "exited", busy: false });
    system(String(e).replace(/^Error: /, ""), "var(--danger)");
    markReady?.();
  }
  if (!history.length) history = await pythonToolsApi.consoleHistory().catch(() => []);
  return ready;
}

export async function stopConsole(): Promise<void> {
  if (state.id && (state.status === "running" || state.status === "starting")) await pythonToolsApi.consoleStop(state.id).catch(() => undefined);
  setState({ status: "exited", busy: false });
}

export async function restartConsole(): Promise<void> {
  await stopConsole();
  setState("status", "idle");
  await startConsole();
}

export function clearConsole(): void {
  lines.length = 0;
  bump();
}

export const toggleVariables = () => setState("showVariables", (v) => !v);

/** Make sure the console runs and shows. */
async function ensure(): Promise<boolean> {
  showToolWindow("pythonConsole");
  if (state.status !== "running") await startConsole();
  return state.status === "running" && !!state.id;
}

/** A line typed at the prompt (Enter). */
export async function submitLine(line: string): Promise<void> {
  if (!state.id || state.status !== "running") return;
  if (state.reading) {
    setState({ reading: false });
    await pythonToolsApi.consoleSend(state.id, "input", { text: line }, `${state.prompt}${line}\n`);
    return;
  }
  const code = [...state.pending, line].join("\n");
  setState({ pending: [...state.pending, line], busy: true });
  await pythonToolsApi.consoleSend(state.id, "exec", { code }, `${state.prompt}${line}\n`);
  if (line.trim()) addHistory(line);
}

function addHistory(entry: string): void {
  history = [...history.filter((h) => h !== entry), entry];
  void pythonToolsApi.consoleHistoryAdd(entry).catch(() => undefined);
}

/** The history, oldest first (Up / Down at the prompt). */
export const consoleHistory = () => history;

/** Run a block of code (Run Selection in Python Console). */
export async function executeCode(code: string): Promise<void> {
  if (!code.trim() || !(await ensure())) return;
  const shown = code
    .replace(/\n+$/, "")
    .split("\n")
    .map((l, i) => `${i === 0 ? primary : more}${l}`)
    .join("\n");
  setState({ busy: true, pending: [] });
  await pythonToolsApi.consoleSend(state.id!, "exec", { code, block: true }, `${shown}\n`);
  addHistory(code.replace(/\n+$/, ""));
}

/** Run File in Python Console: the file runs in the console's namespace. */
export async function runFileInConsole(path: string): Promise<void> {
  if (!(await ensure())) return;
  setState({ busy: true });
  await pythonToolsApi.consoleSend(state.id!, "runfile", { path }, `${primary}runfile('${path}')\n`);
}

export async function loadChildren(expr: string): Promise<void> {
  if (!state.id || state.status !== "running") return;
  await pythonToolsApi.consoleSend(state.id, "children", { expr }).catch(() => undefined);
}

export function toggleExpanded(v: ConsoleVariable): void {
  const open = !state.expanded[v.expr];
  setState("expanded", v.expr, open);
  if (open && !state.children[v.expr]) void loadChildren(v.expr);
}
