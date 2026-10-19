/**
 * Browser-mode settings and plugin registry. Plugins come from `builtinPlugins.json`, a
 * snapshot of the Rust manifests kept current by a cargo test. The user layer survives
 * reloads via localStorage; the demo project's layer lives in memory.
 */
import { emitMock, registerMock } from "@/ipc";
import {
  SETTINGS_CHANGED_EVENT,
  type PluginInfo,
  type SettingsScope,
  type SettingSpec,
  type SettingValue,
} from "@/ipc/settings";
import builtinPlugins from "./builtinPlugins.json";

const STORAGE_KEY = "forge.mock.settings.user";
const DISABLED_KEY = "plugins.disabled";

const plugins: PluginInfo[] = structuredClone(builtinPlugins as unknown as PluginInfo[]);
const allSpecs = (): SettingSpec[] => plugins.flatMap((p) => p.contributions.settings);
const defaults = Object.fromEntries(allSpecs().map((s) => [s.key, s.default])) as Record<string, SettingValue>;

const layers: Record<SettingsScope, Record<string, SettingValue>> = {
  user: (() => {
    try {
      return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    } catch {
      return {};
    }
  })(),
  project: {},
};

const persistUser = () => {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(layers.user));
  } catch {
    /* storage unavailable: keep in memory */
  }
};

function resolve(key: string): { value: SettingValue | null; scope: "default" | SettingsScope } {
  if (key in layers.project) return { value: layers.project[key], scope: "project" };
  if (key in layers.user) return { value: layers.user[key], scope: "user" };
  return { value: defaults[key] ?? null, scope: "default" };
}

const kindOf = (v: unknown) => (Array.isArray(v) ? "array" : typeof v);

export function setSetting(key: string, value: SettingValue | null, scope: SettingsScope): void {
  if (!key || key.split(".").some((p) => !p)) throw new Error(`invalid settings key \`${key}\``);
  const def = defaults[key];
  if (value !== null && def !== undefined && kindOf(def) !== kindOf(value)) {
    throw new Error(`\`${key}\` expects a value like ${JSON.stringify(def)}, got ${JSON.stringify(value)}`);
  }
  const before = JSON.stringify(resolve(key).value);
  if (value === null) delete layers[scope][key];
  else layers[scope][key] = value;
  if (scope === "user") persistUser();
  const after = resolve(key);
  if (JSON.stringify(after.value) !== before) emitMock(SETTINGS_CHANGED_EVENT, { key, ...after });
}

const byId = (id: string) => plugins.find((p) => p.id === id);

/** Same cascade as `PluginRegistry::set_enabled` in Rust. */
export function setPluginEnabled(id: string, enabled: boolean): string[] {
  const plugin = byId(id);
  if (!plugin) throw new Error(`unknown plugin \`${id}\``);
  if (!enabled && plugin.required) throw new Error(`plugin \`${id}\` is required and cannot be disabled`);
  const next = (p: PluginInfo) =>
    enabled ? p.depends : plugins.filter((q) => q.depends.includes(p.id)).map((q) => q.id);
  const affected = [id];
  for (let i = 0; i < affected.length; i++) {
    const p = byId(affected[i]);
    for (const n of p ? next(p) : []) if (!affected.includes(n) && byId(n)) affected.push(n);
  }
  const changed: string[] = [];
  for (const p of plugins) {
    if (affected.includes(p.id) && p.enabled !== enabled) {
      p.enabled = enabled;
      changed.push(p.id);
    }
  }
  setSetting(DISABLED_KEY, plugins.filter((p) => !p.enabled).map((p) => p.id), "user");
  return changed;
}

// Restore disabled plugins from the stored user layer.
for (const id of (layers.user[DISABLED_KEY] as string[] | undefined) ?? []) {
  const p = byId(id);
  if (p && !p.required) p.enabled = false;
}

registerMock("settings_get_all", () => ({
  values: { ...defaults, ...layers.user, ...layers.project },
  user: { ...layers.user },
  project: { ...layers.project },
  hasProject: true,
}));
registerMock("settings_set", ({ key, value, scope }) =>
  setSetting(String(key), (value ?? null) as SettingValue | null, scope as SettingsScope),
);
registerMock("settings_schema", () => plugins.filter((p) => p.enabled).flatMap((p) => p.contributions.settings));
registerMock("plugins_list", () => structuredClone(plugins));
registerMock("plugin_set_enabled", ({ id, enabled }) => setPluginEnabled(String(id), Boolean(enabled)));
