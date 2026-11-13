/** Shared helpers of the Unity and Godot features: running command lines from Rust. */
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { launch } from "@/features/run/runStore";

export const errorText = (e: unknown) => String(e).replace(/^Error: /, "");

/** Run a command line (built in Rust) in the Run tool window; Safe Mode refuses it in the backend. */
export async function runCommandLine(cmd: CommandLine, name: string): Promise<number | null> {
  const cfg: RunConfiguration = { name, type: "gamedev-command", options: { commandLine: JSON.stringify(cmd) } };
  const handle = await launch(cfg);
  return handle ? await handle.exited : null;
}
