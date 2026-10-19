/**
 * Settings and plugins as seen by the UI: effective values, which layer overrides what,
 * the settings schema of enabled plugins and the plugin list. Read values with
 * `getSetting("editor.font.size")`; it is reactive and falls back to the schema default.
 */
import { createStore, reconcile } from "solid-js/store";
import { listen } from "@/ipc";
import {
  SETTINGS_CHANGED_EVENT,
  settingsApi,
  type PluginInfo,
  type SettingsChange,
  type SettingsScope,
  type SettingSpec,
  type SettingValue,
} from "@/ipc/settings";

interface SettingsState {
  loaded: boolean;
  values: Record<string, SettingValue>;
  user: Record<string, SettingValue>;
  project: Record<string, SettingValue>;
  hasProject: boolean;
  schema: SettingSpec[];
  plugins: PluginInfo[];
}

const [settingsState, setState] = createStore<SettingsState>({
  loaded: false,
  values: {},
  user: {},
  project: {},
  hasProject: false,
  schema: [],
  plugins: [],
});

export { settingsState };

export const findSpec = (key: string): SettingSpec | undefined =>
  settingsState.schema.find((s) => s.key === key);

/** Effective value of a setting, falling back to its schema default, then to `fallback`. */
export function getSetting<T extends SettingValue>(key: string, fallback?: T): T {
  const v = settingsState.values[key] ?? findSpec(key)?.default ?? fallback;
  return v as T;
}

/** The layer a setting is written to: project-level settings go to the open project. */
export const scopeFor = (key: string): SettingsScope =>
  findSpec(key)?.project && settingsState.hasProject ? "project" : "user";

/** Set a value only in the UI (no persistence), e.g. to adopt state that already applies. */
export function setLocalSetting(key: string, value: SettingValue): void {
  setState("values", key, value);
}

async function refreshSnapshot(): Promise<void> {
  const snap = await settingsApi.getAll();
  setState("values", reconcile(snap.values));
  setState("user", reconcile(snap.user));
  setState("project", reconcile(snap.project));
  setState("hasProject", snap.hasProject);
}

async function refreshPlugins(): Promise<void> {
  const [schema, plugins] = await Promise.all([settingsApi.schema(), settingsApi.plugins()]);
  setState({ schema, plugins });
}

let listening = false;

export async function loadSettings(): Promise<void> {
  if (!listening) {
    listening = true;
    await listen<SettingsChange>(SETTINGS_CHANGED_EVENT, (c) => {
      if (c.value === null) setState("values", c.key, undefined!);
      else setState("values", c.key, c.value);
    });
  }
  await Promise.all([refreshSnapshot(), refreshPlugins()]);
  setState("loaded", true);
}

/** Persist one setting (`null` resets it to the inherited value). */
export async function saveSetting(key: string, value: SettingValue | null, scope = scopeFor(key)): Promise<void> {
  await settingsApi.set(key, value, scope);
  await refreshSnapshot();
}

/** Persist several settings at once, then refresh the layers once. */
export async function saveSettings(changes: Record<string, SettingValue | null>): Promise<void> {
  for (const [key, value] of Object.entries(changes)) await settingsApi.set(key, value, scopeFor(key));
  await refreshSnapshot();
}

export async function setPluginEnabled(id: string, enabled: boolean): Promise<string[]> {
  const changed = await settingsApi.setPluginEnabled(id, enabled);
  await Promise.all([refreshPlugins(), refreshSnapshot()]);
  return changed;
}
