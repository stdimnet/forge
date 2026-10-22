/**
 * What Run Anything does with a picked item: run or debug a configuration, or turn a typed
 * command into a temporary configuration (resolved in Rust: `php file.php` runs through the
 * project's PHP interpreter, `npm run x` as an npm script, anything else as a shell command
 * in the project directory) and remember it in the history.
 */
import { createSignal } from "solid-js";
import { runToolsApi, type HistoryEntry, type RunAnythingAction } from "@/ipc/runTools";
import { balloonsFor } from "@/keymap";
import { debugConfiguration, launch, runConfiguration } from "@/features/run";
import { findConfiguration, useTemporary } from "@/features/run/configStore";

const showBalloon = balloonsFor("Run Anything");

const [history, setHistory] = createSignal<HistoryEntry[]>([]);
export { history };

export async function loadHistory(): Promise<void> {
  try {
    setHistory(await runToolsApi.history());
  } catch {
    setHistory([]);
  }
}

async function remember(entry: HistoryEntry): Promise<void> {
  try {
    setHistory(await runToolsApi.addHistory(entry));
  } catch (err) {
    console.warn("Run Anything history not saved", err);
  }
}

/** Run (or with `debug` debug) what an item stands for. */
export async function execute(action: RunAnythingAction, debug: boolean): Promise<void> {
  if (action.kind === "insert") return;
  if (action.kind === "configuration") {
    const cfg = findConfiguration(action.name);
    if (!cfg) {
      showBalloon(`Configuration '${action.name}' not found`);
      return;
    }
    void remember({ command: action.name, configuration: true });
    await (debug ? debugConfiguration(cfg) : runConfiguration(cfg));
    return;
  }
  let cfg;
  try {
    cfg = await runToolsApi.resolve(action.command, action.cwd);
  } catch (err) {
    showBalloon("Cannot run command", String(err));
    return;
  }
  void remember({ command: action.command, cwd: action.cwd || undefined });
  const temp = useTemporary(cfg);
  await (debug ? debugConfiguration(temp) : launch(temp));
}
