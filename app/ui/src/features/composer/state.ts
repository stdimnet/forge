/**
 * Composer runtime state: which file is the project's composer.json, running Composer
 * commands in the Run tool window (as `composer-command` configurations the backend builds,
 * so remote projects and composer.phar work) and refreshing what depends on composer.json
 * and composer.lock afterwards.
 */
import { createSignal } from "solid-js";
import type { RunConfiguration } from "@/ipc/run";
import { launch } from "@/features/run/runStore";
import { reloadFile } from "@/state/editor";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";

/** The project's composer.json (Settings | PHP | Composer). */
export function composerJsonPath(): string {
  return (getSetting<string>("php.composer.json", "composer.json") || "composer.json").replace(/^\.\//, "");
}

export function siblingOf(composerJson: string, name: string): string {
  const i = composerJson.lastIndexOf("/");
  return i < 0 ? name : `${composerJson.slice(0, i)}/${name}`;
}

/** composer.json files of the project (not of installed packages). */
export function isComposerJson(path: string): boolean {
  return (path === "composer.json" || path.endsWith("/composer.json")) && !/(^|\/)vendor\//.test(path);
}

export const composerActive = () => isPluginActive("forge.composer");

/** Bumped after a Composer command finished: markers and the dialog's lists reload. */
const [revision, setRevision] = createSignal(0);
export { revision as composerRevision };
export const bumpComposerRevision = () => setRevision((r) => r + 1);

const [running, setRunning] = createSignal(0);
/** A Composer command is running (disables the editor toolbar's actions). */
export const composerRunning = () => running() > 0;

/** The run configuration of `composer <command>` in the folder of composer.json. */
export function composerConfiguration(command: string, composerJson = composerJsonPath()): RunConfiguration {
  const dir = composerJson.includes("/") ? composerJson.slice(0, composerJson.lastIndexOf("/")) : "";
  return {
    name: `composer ${command}`.length > 48 ? `composer ${command.split(" ")[0]}` : `composer ${command}`,
    type: "composer-command",
    workingDirectory: dir,
    options: { command },
  };
}

/**
 * Run `composer <command>` in the Run tool window; resolves with the exit code (null when
 * it could not start). composer.json and composer.lock are reloaded afterwards.
 */
export async function runComposer(command: string, composerJson = composerJsonPath()): Promise<number | null> {
  setRunning((n) => n + 1);
  try {
    const handle = await launch(composerConfiguration(command, composerJson));
    const code = handle ? await handle.exited : null;
    await Promise.all(
      [composerJson, siblingOf(composerJson, "composer.lock")].map((p) => reloadFile(p).catch(() => undefined)),
    );
    bumpComposerRevision();
    return code;
  } finally {
    setRunning((n) => n - 1);
  }
}

/** `composer require` arguments: `vendor/package:^1.0 [--dev]`. */
export function requireCommand(name: string, constraint: string, dev: boolean): string {
  const c = constraint.trim();
  const spec = c ? `${name}:${c}` : name;
  return `require ${/\s/.test(spec) ? `"${spec}"` : spec}${dev ? " --dev" : ""}`;
}
