/**
 * RubyForge state in the UI: whether the Ruby plugin is active, the Ruby status
 * (`ruby_status`), the bundle (`ruby_bundle`), the Rake tasks (`ruby_rake_tasks`), and
 * running Bundler / gem / Rake command lines in the Run tool window through the hidden
 * `ruby-command` configuration type.
 */
import { createSignal } from "solid-js";
import { rubyApi, type Bundle, type RakeTasks, type RubyStatus } from "@/ipc/ruby";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { launch } from "@/features/run/runStore";
import { balloonsFor } from "@/keymap/dispatch";
import { isPluginActive } from "@/state/project";

export const PLUGIN = "forge.ruby";
export const rubyActive = () => isPluginActive(PLUGIN);
export const showRubyBalloon = balloonsFor("Ruby");
export const errorText = (e: unknown) => String(e).replace(/^Error: /, "");

const [status, setStatus] = createSignal<RubyStatus | null>(null);
export { status as rubyStatus };

export async function refreshRubyStatus(refresh = false): Promise<RubyStatus | null> {
  try {
    const s = await rubyApi.status(refresh);
    setStatus(s);
    return s;
  } catch {
    return null;
  }
}

const [bundle, setBundle] = createSignal<Bundle | null>(null);
export { bundle as rubyBundle };

export async function refreshBundle(): Promise<Bundle | null> {
  try {
    const b = await rubyApi.bundle();
    setBundle(b);
    return b;
  } catch {
    setBundle(null);
    return null;
  }
}

const [rake, setRake] = createSignal<RakeTasks | null>(null);
const [rakeLoading, setRakeLoading] = createSignal(false);
export { rake as rakeTasks, rakeLoading };

/** Load the Rake tasks (`refresh`: ask Rake again). */
export async function refreshRake(refresh = false): Promise<RakeTasks | null> {
  setRakeLoading(true);
  try {
    const t = await rubyApi.rakeTasks(refresh);
    setRake(t);
    return t;
  } catch (e) {
    setRake({ tasks: [], source: "files", error: errorText(e) });
    return null;
  } finally {
    setRakeLoading(false);
  }
}

/** `bundle install` for `/home/anna/.rbenv/versions/3.3.6/bin/bundle install`. */
export function commandName(cmd: CommandLine): string {
  const program = cmd.program.replace(/\\/g, "/").split("/").pop()?.replace(/\.(exe|bat|cmd)$/, "") ?? cmd.program;
  const args = cmd.args.filter((a) => !a.startsWith("--install-dir") && !a.startsWith("--bindir"));
  const line = [program, ...args].join(" ");
  return line.length > 60 ? `${line.slice(0, 57)}…` : line;
}

/** Run a command line in the Run tool window; resolves with its exit code. */
export async function runRubyCommand(cmd: CommandLine, name = commandName(cmd)): Promise<number | null> {
  const cfg: RunConfiguration = { name, type: "ruby-command", options: { commandLine: JSON.stringify(cmd) } };
  const handle = await launch(cfg);
  return handle ? await handle.exited : null;
}

/** Interpreter label for the status bar and settings: `rbenv: 3.3.6`. */
export const interpreterLabel = (s: RubyStatus | null) => s?.chosen?.interpreter.name ?? "No Ruby interpreter";
