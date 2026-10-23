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
  /** Settings for New Projects: defaults of project-level settings. */
  newProjects: Record<string, SettingValue>;
  hasProject: boolean;
  schema: SettingSpec[];
  plugins: PluginInfo[];
}

const [settingsState, setState] = createStore<SettingsState>({
  loaded: false,
  values: {},
  user: {},
  project: {},
  newProjects: {},
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

/**
 * The layer a setting is written to: project-level settings go to the open project, or to
 * the Settings for New Projects when no project is open or `forNewProjects` is set (File |
 * New Projects Setup | Settings for New Projects).
 */
export function scopeFor(key: string, forNewProjects = false): SettingsScope {
  if (!findSpec(key)?.project) return "user";
  return settingsState.hasProject && !forNewProjects ? "project" : "newProjects";
}

/** The value a setting has for new projects: their default, else the user's or the schema's. */
export function newProjectValue<T extends SettingValue>(key: string): T {
  const v = settingsState.newProjects[key] ?? settingsState.user[key] ?? findSpec(key)?.default;
  return v as T;
}

/** The value `key` would have without the layer `scope` (what removing the override gives). */
export function inheritedValue(key: string, scope: SettingsScope): SettingValue | undefined {
  const layers: SettingsScope[] = ["project", "newProjects", "user"];
  for (const layer of layers.slice(layers.indexOf(scope) + 1)) {
    const v = settingsState[layer][key];
    if (v !== undefined) return v;
  }
  return findSpec(key)?.default;
}

/** Set a value only in the UI (no persistence), e.g. to adopt state that already applies. */
export function setLocalSetting(key: string, value: SettingValue): void {
  setState("values", key, value);
}

async function refreshSnapshot(): Promise<void> {
  const snap = await settingsApi.getAll();
  setState("values", reconcile(snap.values));
  setState("user", reconcile(snap.user));
  setState("project", reconcile(snap.project));
  setState("newProjects", reconcile(snap.newProjects ?? {}));
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

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Persist several settings at once, then refresh the layers once. A value equal to what the
 * layers below give removes the override instead of storing a copy.
 */
export async function saveSettings(changes: Record<string, SettingValue | null>, forNewProjects = false): Promise<void> {
  for (const [key, value] of Object.entries(changes)) {
    const scope = scopeFor(key, forNewProjects);
    const stored = value !== null && same(value, inheritedValue(key, scope)) ? null : value;
    if (stored === null && settingsState[scope][key] === undefined) continue;
    await settingsApi.set(key, stored, scope);
  }
  await refreshSnapshot();
}

export async function setPluginEnabled(id: string, enabled: boolean): Promise<string[]> {
  const changed = await settingsApi.setPluginEnabled(id, enabled);
  await Promise.all([refreshPlugins(), refreshSnapshot()]);
  return changed;
}
