/**
 * RustForge state in the UI: whether the Rust plugin is active, the toolchain status
 * (`rust_status`), the Cargo workspace (`cargo_workspace`), and running cargo / rustup
 * command lines in the Run tool window through the hidden `cargo-command` configuration
 * type.
 */
import { createSignal } from "solid-js";
import { rustApi, type CargoWorkspace, type RustStatus } from "@/ipc/rust";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { launch } from "@/features/run/runStore";
import { balloonsFor } from "@/keymap/dispatch";
import { isPluginActive } from "@/state/project";

export const PLUGIN = "forge.rust";
export const rustActive = () => isPluginActive(PLUGIN);
export const showRustBalloon = balloonsFor("Rust");

const [status, setStatus] = createSignal<RustStatus | null>(null);
export { status as rustStatus };

export async function refreshRustStatus(refresh = false): Promise<RustStatus | null> {
  try {
    const s = await rustApi.status(refresh);
    setStatus(s);
    return s;
  } catch {
    return null;
  }
}

const [workspace, setWorkspace] = createSignal<CargoWorkspace | null>(null);
const [workspaceError, setWorkspaceError] = createSignal<string | null>(null);
const [loading, setLoading] = createSignal(false);
export { workspace as cargoWorkspace, workspaceError as cargoWorkspaceError, loading as cargoLoading };

/** Load the Cargo workspace (`refresh`: run `cargo metadata` again). */
export async function refreshCargo(refresh = false): Promise<CargoWorkspace | null> {
  setLoading(true);
  try {
    const ws = await rustApi.workspace(refresh);
    setWorkspace(ws);
    setWorkspaceError(null);
    return ws;
  } catch (e) {
    setWorkspaceError(String(e).replace(/^Error: /, ""));
    return null;
  } finally {
    setLoading(false);
  }
}

/** `cargo build -p shop-core` for `/home/anna/.cargo/bin/cargo build -p shop-core`. */
export function commandName(cmd: CommandLine): string {
  const program = cmd.program.replace(/\\/g, "/").split("/").pop()?.replace(/\.exe$/, "") ?? cmd.program;
  const args = cmd.args.filter((a, i, all) => a !== "--manifest-path" && all[i - 1] !== "--manifest-path");
  const line = [program, ...args].join(" ");
  return line.length > 60 ? `${line.slice(0, 57)}…` : line;
}

/** Run a command line in the Run tool window; resolves with its exit code. */
export async function runRustCommand(cmd: CommandLine, name = commandName(cmd)): Promise<number | null> {
  const cfg: RunConfiguration = { name, type: "cargo-command", options: { commandLine: JSON.stringify(cmd) } };
  const handle = await launch(cfg);
  return handle ? await handle.exited : null;
}
