/**
 * GoForge state in the UI: whether the Go plugin is active, the project's SDK and tools
 * (`go_status`), and running Go command lines (module commands, go generate, tool
 * installs) in the Run tool window through the hidden `go-command` configuration type.
 */
import { createSignal } from "solid-js";
import { goApi, type GoStatus, type GoToolId } from "@/ipc/go";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { launch } from "@/features/run/runStore";
import { balloonsFor } from "@/keymap/dispatch";
import { isPluginActive } from "@/state/project";

export const PLUGIN = "forge.go";
export const goActive = () => isPluginActive(PLUGIN);
export const showGoBalloon = balloonsFor("Go");

const [status, setStatus] = createSignal<GoStatus | null>(null);
export { status as goStatus };

export async function refreshGoStatus(refresh = false): Promise<GoStatus | null> {
  try {
    const s = await goApi.status(refresh);
    setStatus(s);
    return s;
  } catch {
    return null;
  }
}

/** `go mod tidy` for `/usr/local/go/bin/go mod tidy`. */
export function commandName(cmd: CommandLine): string {
  const program = cmd.program.replace(/\\/g, "/").split("/").pop() ?? cmd.program;
  const line = [program, ...cmd.args.filter((a) => !a.startsWith("^//go:generate"))].join(" ");
  return line.length > 60 ? `${line.slice(0, 57)}…` : line;
}

/** Run a command line in the Run tool window; resolves with its exit code. */
export async function runGoCommand(cmd: CommandLine, name = commandName(cmd)): Promise<number | null> {
  const cfg: RunConfiguration = { name, type: "go-command", options: { commandLine: JSON.stringify(cmd) } };
  const handle = await launch(cfg);
  return handle ? await handle.exited : null;
}

/** Install a Go tool with the project's Go toolchain (`go install <module>@<pinned>`). */
export async function installTool(tool: GoToolId): Promise<boolean> {
  try {
    const cmd = await goApi.toolInstallCommand(tool);
    const code = await runGoCommand(cmd);
    await refreshGoStatus(true);
    if (code === 0) showGoBalloon(`${cmd.args[1] ?? tool} is installed`, "", 4000);
    return code === 0;
  } catch (e) {
    showGoBalloon(`Cannot install ${tool}`, String(e).replace(/^Error: /, ""), 8000);
    return false;
  }
}
