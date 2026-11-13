/**
 * Browser-mode settings and plugin registry. Plugins come from `builtinPlugins.json`, a
 * snapshot of the Rust manifests kept current by a cargo test. The user layer survives
 * reloads via localStorage; the demo project's layer lives in memory.
 */
import { emitMock, registerMock } from "@/ipc";
import {
  SETTINGS_CHANGED_EVENT,
  type LaunchProfile,
  type PluginInfo,
  type SettingsScope,
  type SettingSpec,
  type SettingValue,
} from "@/ipc/settings";
import { callWasm, callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";
import builtinPlugins from "./builtinPlugins.json";

// Commands wait for the WebAssembly build, so validation below can call it synchronously.
let wasm: ForgeWasm | undefined;
const ready = forgeWasm().then((w) => {
  wasm = w;
  applyLaunchProfile(w);
  return w;
});

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
  newProjects: {},
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
  if (key in layers.newProjects) return { value: layers.newProjects[key], scope: "newProjects" };
  if (key in layers.user) return { value: layers.user[key], scope: "user" };
  return { value: defaults[key] ?? allSpecs().find((s) => s.key === key)?.default ?? null, scope: "default" };
}

/** Effective value of a setting (for other mocks). */
export const mockSettingValue = (key: string): SettingValue | null => resolve(key).value;

/** Every effective setting value (for mocks that pass the settings to Rust). */
export const mockSettingValues = (): Record<string, SettingValue> => ({
  ...defaults,
  ...layers.user,
  ...layers.newProjects,
  ...layers.project,
});

/** Copies of the user and project layers (the demo project is always open). */
export const mockSettingLayers = (): Record<SettingsScope, Record<string, SettingValue>> => ({
  user: { ...layers.user },
  newProjects: { ...layers.newProjects },
  project: { ...layers.project },
});

/** Whether the user layer stores `key`. */
export const hasUserSetting = (key: string): boolean => key in layers.user;

/** The schema check of `settings_set`, run by the same Rust code. */
function validate(key: string, value: SettingValue): string | null {
  const spec = allSpecs().find((s) => s.key === key);
  return spec && wasm ? callWasmSync<string | null>(wasm.settings_validate, { spec, value }) : null;
}

export function setSetting(key: string, value: SettingValue | null, scope: SettingsScope): void {
  if (!key || key.split(".").some((p) => !p)) throw new Error(`invalid settings key \`${key}\``);
  if (scope === "newProjects" && !allSpecs().find((s) => s.key === key)?.project) {
    throw new Error(`\`${key}\` is not a project setting and has no value for new projects`);
  }
  const problem = value === null ? null : validate(key, value);
  if (problem) throw new Error(problem);
  const before = JSON.stringify(resolve(key).value);
  if (value === null) delete layers[scope][key];
  else layers[scope][key] = value;
  if (scope === "user") persistUser();
  const after = resolve(key);
  if (JSON.stringify(after.value) !== before) emitMock(SETTINGS_CHANGED_EVENT, { key, ...after });
}

const byId = (id: string) => plugins.find((p) => p.id === id);

/**
 * `?packs=phpforge,webforge` (or `none`) starts browser mode like `forge --packs …`: the
 * plugins of the other packs are off and cannot be enabled. The same Rust code decides
 * which (`forge_plugin_api::profile::resolve`).
 */
let launchProfile: LaunchProfile = { all: true, packs: [], hidden: [], unknown: [] };
function applyLaunchProfile(w: ForgeWasm): void {
  const spec = typeof location !== "undefined" ? new URLSearchParams(location.search).get("packs") : null;
  if (!spec) return;
  launchProfile = callWasmSync<LaunchProfile>(w.plugins_launch_profile, {
    spec,
    plugins: plugins.map(({ id, pack, depends, required }) => ({ id, pack, depends, required })),
  });
  for (const p of plugins) if (launchProfile.hidden.includes(p.id)) p.enabled = false;
}

/** The registry itself, for the third-party plugin mock (installs add entries). */
export const mockPluginRegistry = (): PluginInfo[] => plugins;

/** Test frameworks the enabled plugins declare (`contributions.testFrameworks`). */
export const mockTestFrameworkSpecs = () => plugins.filter((p) => p.enabled).flatMap((p) => p.contributions.testFrameworks ?? []);

/** Inspections and tool mappings of the enabled plugins (`contributions.inspections`). */
export function mockInspectionContributions(): { inspections: unknown[]; mappings: unknown[] } {
  const enabled = plugins.filter((p) => p.enabled).map((p) => p.contributions as unknown as { inspections?: unknown[]; inspectionMappings?: unknown[] });
  return { inspections: enabled.flatMap((c) => c.inspections ?? []), mappings: enabled.flatMap((c) => c.inspectionMappings ?? []) };
}

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
  const hidden = enabled ? affected.find((a) => launchProfile.hidden.includes(a)) : undefined;
  if (hidden) throw new Error(`plugin \`${hidden}\` is not part of the packs Forge was started with`);
  const changed: string[] = [];
  for (const p of plugins) {
    if (affected.includes(p.id) && p.enabled !== enabled) {
      p.enabled = enabled;
      changed.push(p.id);
    }
  }
  // Plugins the launch profile turned off keep what the stored list says about them.
  const saved = (layers.user[DISABLED_KEY] as string[] | undefined) ?? [];
  const disabled = plugins.filter((p) => (launchProfile.hidden.includes(p.id) ? saved.includes(p.id) : !p.enabled));
  setSetting(DISABLED_KEY, disabled.map((p) => p.id), "user");
  return changed;
}

// Restore disabled plugins from the stored user layer.
for (const id of (layers.user[DISABLED_KEY] as string[] | undefined) ?? []) {
  const p = byId(id);
  if (p && !p.required) p.enabled = false;
}

registerMock("settings_get_all", () => ({
  values: mockSettingValues(),
  user: { ...layers.user },
  project: { ...layers.project },
  newProjects: { ...layers.newProjects },
  hasProject: true,
}));
registerMock("settings_set", async ({ key, value, scope }) => {
  await ready;
  setSetting(String(key), (value ?? null) as SettingValue | null, scope as SettingsScope);
});
// Manifests in the IDE language, like the desktop commands (`forge_plugin_api::l10n`).
registerMock("settings_schema", async () => {
  await ready;
  const specs = plugins.filter((p) => p.enabled).flatMap((p) => p.contributions.settings);
  return wasm ? callWasmSync<SettingSpec[]>(wasm.i18n_localize_settings, { specs, plugins }) : specs;
});
registerMock("plugins_list", async () => {
  await ready;
  return wasm ? callWasmSync<PluginInfo[]>(wasm.i18n_localize_plugins, plugins) : structuredClone(plugins);
});
registerMock("launch_profile", async () => {
  await ready;
  return structuredClone(launchProfile);
});
registerMock("settings_search", async ({ request }) => callWasm((await ready).settings_search, request));
registerMock("settings_validate", async ({ key, value }) => {
  await ready;
  return validate(String(key), value as SettingValue);
});
registerMock("plugin_set_enabled", ({ id, enabled }) => setPluginEnabled(String(id), Boolean(enabled)));
