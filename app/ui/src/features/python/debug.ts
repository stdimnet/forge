/**
 * Debugging Python with debugpy (MIT) through the shared Debug tool window:
 *
 * - Debug (Shift+F9) on a Python, pytest or Unittests configuration makes sure the
 *   interpreter has debugpy (installing it with the user's consent), starts the run with
 *   `-m debugpy --listen <host>:<port> --wait-for-client` after the interpreter options
 *   and attaches to it as a DAP server (`python_debug_plan`: local interpreters listen on
 *   127.0.0.1; Docker, SSH, WSL and run targets on every interface, with the port published
 *   or forwarded and the path mappings for breakpoints).
 * - The `python` adapter of the manifest (`python -m debugpy.adapter`) serves Attach to
 *   Process and Attach to DAP Server; the debug console evaluates in the selected frame.
 */
import { registerDebugLauncher, type DebugLaunchContext } from "@/features/run";
import { registerDebugAdapter } from "@/features/debug/adapters";
import { dapApi, type LaunchRequest } from "@/features/debug/dapApi";
import { askConfirmWithCheckbox } from "@/features/contextMenu/dialogs";
import { pythonApi } from "@/ipc/python";
import { pythonToolsApi, type DebugPlan } from "@/ipc/pythonTools";
import type { RunConfiguration } from "@/ipc/run";
import { balloonsFor } from "@/keymap/dispatch";
import { showToolWindow } from "@/state/layout";
import { getSetting, saveSettings } from "@/state/settings";
import { commandName, runCommandLine } from "./state";

const showDebuggerBalloon = balloonsFor("Debugger");

/** Configuration types debugpy debugs. */
export const PYTHON_DEBUG_TYPES = new Set(["python", "pytest", "unittest"]);

registerDebugAdapter({
  type: "python",
  label: "Python (debugpy)",
  languages: ["py", "pyw"],
});

/** `-m debugpy --listen <host>:<port> --wait-for-client` (like `pyforge_testing::command::debugpy_args`). */
export function debugpyArgs(plan: Pick<DebugPlan, "listenHost" | "port" | "subProcess">): string[] {
  const args = ["-m", "debugpy", "--listen", `${plan.listenHost}:${plan.port}`, "--wait-for-client"];
  if (!plan.subProcess) args.push("--configure-subProcess", "false");
  return args;
}

/** debugpy's attach arguments of a debug run. */
export function attachRequest(cfg: RunConfiguration, plan: DebugPlan): LaunchRequest {
  return {
    mode: "attachServer",
    adapter: "python",
    host: plan.connectHost,
    port: plan.port,
    name: cfg.name,
    configuration: {
      justMyCode: plan.justMyCode,
      subProcess: plan.subProcess,
      showReturnValue: true,
      // The program's output already streams into the Run console.
      redirectOutput: false,
      ...(plan.pathMappings.length ? { pathMappings: plan.pathMappings } : {}),
    },
  };
}

/** debugpy in the configuration's interpreter: installed, or installed now with consent. */
export async function ensureDebugpy(interpreter?: string): Promise<boolean> {
  const status = await pythonToolsApi.debugpyStatus(interpreter).catch((e) => {
    showDebuggerBalloon("Cannot debug", String(e).replace(/^Error: /, ""), 8000);
    return null;
  });
  if (!status) return false;
  if (status.installed) return true;
  if (!getSetting<boolean>("python.debugger.installWithoutAsking", false)) {
    const answer = await askConfirmWithCheckbox({
      title: "Install debugpy",
      message: `Debugging needs debugpy (MIT licence) in ${status.interpreter}. Install it now with the environment's package manager?`,
      confirm: "Install",
      checkbox: "Install without asking next time",
    });
    if (!answer.ok) return false;
    if (answer.checked) await saveSettings({ "python.debugger.installWithoutAsking": true });
  }
  const cmd = await pythonApi.packagesCommand("install", ["debugpy"], interpreter);
  const code = await runCommandLine(commandName(cmd), cmd);
  if (code !== 0) {
    showDebuggerBalloon("debugpy was not installed", "See the Run tool window for the package manager's output.", 8000);
    return false;
  }
  return true;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Attach to a debug run once debugpy listens (it starts before the program). */
async function attach(request: LaunchRequest, exited: () => boolean, timeoutMs = 30_000): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  let last = "";
  while (Date.now() < deadline && !exited()) {
    try {
      await dapApi.start(request);
      return true;
    } catch (e) {
      last = String(e).replace(/^Error: /, "");
      await sleep(300);
    }
  }
  if (!exited()) showDebuggerBalloon("Cannot attach to debugpy", last || "It did not listen in time.", 8000);
  return false;
}

/** Debug a Python, pytest or Unittests configuration. */
export async function debugPython(cfg: RunConfiguration, ctx: DebugLaunchContext): Promise<void> {
  if (!(await ensureDebugpy(cfg.interpreter))) return;
  let plan: DebugPlan;
  try {
    plan = await pythonToolsApi.debugPlan(cfg);
  } catch (e) {
    showDebuggerBalloon(`Cannot debug '${cfg.name}'`, String(e).replace(/^Error: /, ""), 8000);
    return;
  }
  const handle = await ctx.run({ interpreterArgs: debugpyArgs(plan), debugPort: { port: plan.port, direction: "ideConnects" } });
  if (!handle) return;
  let done = false;
  void handle.exited.then(() => (done = true));
  showToolWindow("debug");
  await attach(attachRequest(cfg, plan), () => done);
}

registerDebugLauncher({
  id: "debugpy",
  canDebug: (cfg) => PYTHON_DEBUG_TYPES.has(cfg.type),
  launch: debugPython,
});
