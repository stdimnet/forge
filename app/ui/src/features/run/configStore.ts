/**
 * The project's Run/Debug configurations, the one selected in the toolbar, and temporary
 * configurations created from gutter icons or "Run Context Configuration" (kept in memory,
 * at most five, like PHPStorm).
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { runApi, type RunConfiguration } from "@/ipc/run";
import { workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";

const MAX_TEMPORARY = 5;

const [configurations, setConfigurations] = createSignal<RunConfiguration[]>([]);
const [temporary, setTemporary] = createSignal<RunConfiguration[]>([]);
const [selectedName, setSelectedName] = createSignal<string | null>(null);
const [loaded, setLoaded] = createSignal(false);

export { configurations, temporary, selectedName, loaded };

export const isTemporary = (cfg: RunConfiguration) => temporary().some((t) => t.name === cfg.name);

/** Saved and temporary configurations, as listed in the toolbar dropdown. */
export const allConfigurations = () => [...configurations(), ...temporary()];

export const findConfiguration = (name: string | null | undefined) =>
  name ? allConfigurations().find((c) => c.name === name) : undefined;

export const selectedConfiguration = () => findConfiguration(selectedName()) ?? configurations()[0] ?? temporary()[0];

export async function loadConfigurations(): Promise<void> {
  if (!workspace()) {
    setConfigurations([]);
    setTemporary([]);
    setSelectedName(null);
    return;
  }
  try {
    const file = await runApi.loadConfigurations();
    setConfigurations(file.configurations);
    setSelectedName(file.selected ?? file.configurations[0]?.name ?? null);
  } catch (err) {
    showBalloon("Run configurations could not be loaded", String(err));
  } finally {
    setLoaded(true);
  }
}

async function persist(): Promise<void> {
  const selected = selectedName();
  try {
    await runApi.saveConfigurations({
      configurations: configurations(),
      // Temporary configurations are not saved; neither is selecting one.
      selected: selected && configurations().some((c) => c.name === selected) ? selected : undefined,
    });
  } catch (err) {
    showBalloon("Run configurations could not be saved", String(err));
  }
}

/** Replace all saved configurations (Run/Debug Configurations dialog: OK / Apply). */
export async function saveConfigurations(list: RunConfiguration[], selected?: string | null): Promise<void> {
  setConfigurations(list);
  // A temporary configuration that was edited and saved becomes permanent.
  setTemporary((t) => t.filter((x) => !list.some((c) => c.name === x.name)));
  if (selected !== undefined) setSelectedName(selected);
  else if (!findConfiguration(selectedName())) setSelectedName(list[0]?.name ?? null);
  await persist();
}

export function selectConfiguration(name: string): void {
  setSelectedName(name);
  if (configurations().some((c) => c.name === name)) void persist();
}

/** A unique name based on `base`: "UserTest", "UserTest (1)", ... */
export function uniqueName(base: string, taken: string[] = allConfigurations().map((c) => c.name)): string {
  if (!taken.includes(base)) return base;
  for (let i = 1; ; i++) {
    const name = `${base} (${i})`;
    if (!taken.includes(name)) return name;
  }
}

/**
 * Use a temporary configuration: an existing saved or temporary one with the same type and
 * options is reused, otherwise it is added (dropping the oldest beyond five) and selected.
 */
export function useTemporary(cfg: RunConfiguration): RunConfiguration {
  const same = (a: RunConfiguration) =>
    a.type === cfg.type && JSON.stringify(a.options ?? {}) === JSON.stringify(cfg.options ?? {});
  const existing = allConfigurations().find(same);
  if (existing) {
    setSelectedName(existing.name);
    return existing;
  }
  const named = { ...cfg, name: uniqueName(cfg.name) };
  setTemporary((t) => [...t, named].slice(-MAX_TEMPORARY));
  setSelectedName(named.name);
  return named;
}

/** Turn a temporary configuration into a saved one ("Save Configuration"). */
export async function saveTemporary(name: string): Promise<void> {
  const cfg = temporary().find((t) => t.name === name);
  if (!cfg) return;
  await saveConfigurations([...configurations(), cfg], name);
}

if (typeof window !== "undefined") {
  createRoot(() => createEffect(on(() => workspace()?.root, () => void loadConfigurations())));
}
