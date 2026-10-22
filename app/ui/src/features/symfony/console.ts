/** Console command runs: started from the popup or a gutter icon, shown as tool window tabs. */
import { createSignal } from "solid-js";
import { symfonyApi, type ConsoleEvent } from "@/ipc/symfony";
import { showToolWindow } from "@/state/layout";

export interface ConsoleLine {
  text: string;
  stderr: boolean;
}

export interface ConsoleRun {
  id: string;
  command: string;
  args: string;
  commandLine: string;
  lines: ConsoleLine[];
  running: boolean;
  exitCode: number | null;
}

const [runs, setRuns] = createSignal<ConsoleRun[]>([]);
/** Selected tool window tab: "routes", "services", "parameters" or a run id. */
const [activeTab, setActiveTab] = createSignal<string>("routes");

export { runs, activeTab, setActiveTab };

let seq = 0;
let listening = false;

function update(id: string, fn: (r: ConsoleRun) => ConsoleRun) {
  setRuns((all) => all.map((r) => (r.id === id ? fn(r) : r)));
}

function onEvent(e: ConsoleEvent) {
  switch (e.type) {
    case "started":
      update(e.runId, (r) => ({ ...r, commandLine: e.commandLine }));
      break;
    case "output":
      update(e.runId, (r) => ({ ...r, lines: [...r.lines, { text: e.text, stderr: e.stderr }] }));
      break;
    case "exit":
      update(e.runId, (r) => ({ ...r, running: false, exitCode: e.code }));
      break;
  }
}

/** `cache:clear --no-warmup` -> command and argument string. */
export function splitCommandLine(input: string): { command: string; args: string } {
  const trimmed = input.trim();
  const space = trimmed.search(/\s/);
  return space < 0 ? { command: trimmed, args: "" } : { command: trimmed.slice(0, space), args: trimmed.slice(space + 1).trim() };
}

/** Run `bin/console <command> <args>` and show its output in the Symfony tool window. */
export async function runConsoleCommand(command: string, args = ""): Promise<void> {
  if (!listening) {
    listening = true;
    await symfonyApi.onConsole(onEvent);
  }
  const id = `run-${Date.now()}-${++seq}`;
  const run: ConsoleRun = {
    id,
    command,
    args,
    commandLine: `php bin/console ${command}${args ? ` ${args}` : ""}`,
    lines: [],
    running: true,
    exitCode: null,
  };
  // Keep the last five runs, like PHPStorm's run tabs.
  setRuns((all) => [...all.filter((r) => r.running || all.indexOf(r) >= all.length - 4), run]);
  setActiveTab(id);
  showToolWindow("symfony");
  try {
    await symfonyApi.runConsole(command, args, id);
  } catch (e) {
    update(id, (r) => ({ ...r, running: false, exitCode: -1, lines: [...r.lines, { text: String(e), stderr: true }] }));
  }
}

export function stopRun(id: string): void {
  void symfonyApi.cancelConsole(id);
}

export function rerun(run: ConsoleRun): void {
  void runConsoleCommand(run.command, run.args);
}

export function closeRun(id: string): void {
  const run = runs().find((r) => r.id === id);
  if (run?.running) stopRun(id);
  setRuns((all) => all.filter((r) => r.id !== id));
  if (activeTab() === id) setActiveTab("routes");
}
