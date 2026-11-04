/**
 * Browser-mode third-party plugins and Settings Sync. The SDK's Word Count example
 * (`sdk/examples/word-count`) is installed and runs in the real sandboxed host; Install
 * Plugin from Disk offers its 1.1.0 build, and the demo marketplace index is evaluated by
 * the same Rust code as WebAssembly (`forge-plugins`). Nothing is downloaded or written.
 */
import { emitMock, registerMock } from "@/ipc";
import type {
  FetchRequest,
  InstalledPlugin,
  Inspection,
  MarketplaceEntry,
  PermissionLine,
  PluginPermissions,
  SyncReport,
  ThirdPartyManifest,
} from "@/ipc/plugins";
import type { PluginInfo, SettingSpec } from "@/ipc/settings";
import { callWasmSync, forgeWasm } from "@/wasm";
import wordCountManifest from "../../../../sdk/examples/word-count/forge-plugin.json";
import wordCountBundle from "../../../../sdk/examples/word-count/dist/plugin.js?raw";
import { mockPluginRegistry, mockSettingValue, setSetting } from "./settingsMock";

const FORGE_VERSION = "0.1.0";
const CHANGED = "plugins://changed";
const PLACEHOLDER = "https://plugins.forge.example/index.json";

const manifest = (version = "1.0.0"): ThirdPartyManifest => ({
  ...(structuredClone(wordCountManifest) as unknown as ThirdPartyManifest),
  version,
});

const lines = async (p: PluginPermissions): Promise<PermissionLine[]> =>
  callWasmSync<PermissionLine[]>((await forgeWasm()).plugins_permission_lines, p);

const installed: InstalledPlugin[] = [];
const bundles = new Map<string, string>();

async function install(m: ThirdPartyManifest, bundle: string, source: InstalledPlugin["source"]): Promise<InstalledPlugin> {
  const old = installed.findIndex((p) => p.manifest.id === m.id);
  const p: InstalledPlugin = {
    manifest: m,
    enabled: old >= 0 ? installed[old].enabled : true,
    granted: m.permissions,
    source,
    sha256: "0".repeat(64),
    installedAt: Date.now() / 1000,
    permissionLines: await lines(m.permissions),
    needsTrust: !!m.permissions.process?.length,
  };
  if (old >= 0) installed.splice(old, 1, p);
  else installed.push(p);
  bundles.set(m.id, bundle);
  // The registry entry Settings | Plugins lists (as `forge_plugins::store::descriptor`).
  const registry = mockPluginRegistry();
  const entry = {
    id: m.id,
    name: m.name,
    version: m.version,
    pack: "thirdParty",
    description: m.description,
    vendor: m.publisher,
    depends: [],
    required: false,
    enabled: p.enabled,
    contributions: {
      languages: [],
      fileTypes: m.contributes.languages ?? [],
      commands: m.contributes.commands ?? [],
      keybindings: m.contributes.keybindings ?? [],
      toolWindows: m.contributes.toolWindows ?? [],
      settings: (m.contributes.settings ?? []) as SettingSpec[],
      lspServers: [],
      runConfigurationTypes: [],
      aiProviders: [],
      chatAgents: [],
      codeGenerators: [],
      mainMenu: [],
    },
  } as unknown as PluginInfo;
  const i = registry.findIndex((r) => r.id === m.id);
  if (i >= 0) registry.splice(i, 1, entry);
  else registry.unshift(entry);
  return p;
}

const ready = install(manifest(), wordCountBundle, { kind: "disk", file: "~/Downloads/example.word-count-1.0.0.forgeplugin" });

/** The demo marketplace: what an index at the configured URL could list. */
function demoIndex(): string {
  const v = (version: string, permissions: PluginPermissions, extra: Record<string, unknown> = {}) => ({
    version,
    engines: { forge: ">=0.1.0" },
    url: `files/${version}.forgeplugin`,
    sha256: "9f2c4e5a1b7d3c8e6f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c0d2e",
    size: 4211,
    permissions,
    released: "2026-10-28",
    ...extra,
  });
  return JSON.stringify({
    formatVersion: 1,
    name: "Demo marketplace (browser mode)",
    plugins: [
      {
        id: "example.word-count",
        name: "Word Count",
        publisher: "Forge SDK Examples",
        description: wordCountManifest.description,
        tags: ["editor", "writing"],
        downloads: 1840,
        versions: [
          v("1.0.0", { editor: ["read"] }),
          v("1.1.0", { editor: ["read"] }, { changes: "Reading time in the tool window." }),
        ],
      },
      {
        id: "acme.todo-sync",
        name: "TODO Sync",
        publisher: "Acme",
        description: "Turns TODO comments into issues of your tracker and links them back.",
        tags: ["todo", "issues"],
        downloads: 12400,
        versions: [v("2.3.0", { network: ["api.acme.dev"], fs: { read: ["${project}"] }, editor: ["read"] })],
      },
      {
        id: "acme.changelog",
        name: "Changelog Helper",
        publisher: "Acme",
        description: "Writes CHANGELOG.md entries from the commits since the last tag.",
        tags: ["git", "release"],
        downloads: 5300,
        versions: [v("1.4.2", { process: ["git"], fs: { read: ["${project}"], write: ["${project}"] } })],
      },
      {
        id: "lumen.color-picker",
        name: "Lumen Color Picker",
        publisher: "Lumen",
        description: "A color picker tool window with palettes stored per user.",
        tags: ["css", "colors"],
        downloads: 960,
        versions: [v("0.9.0", { fs: { write: ["${pluginData}"] }, editor: ["read", "write"] })],
      },
      {
        id: "retro.future",
        name: "Future Feature",
        publisher: "Retro",
        description: "Needs a newer Forge.",
        downloads: 12,
        versions: [v("3.0.0", {}, { engines: { forge: ">=9.0.0" } })],
      },
    ],
  });
}

async function marketplace(query: string) {
  await ready;
  const url = (mockSettingValue("plugins.marketplace.url") as string | null) || PLACEHOLDER;
  const out = callWasmSync<{ name: string; entries: MarketplaceEntry[]; warnings: string[] } | { error: string }>(
    (await forgeWasm()).plugins_marketplace,
    { index: demoIndex(), forgeVersion: FORGE_VERSION, installed: installed.map((p) => ({ id: p.manifest.id, version: p.manifest.version })), query },
  );
  if ("error" in out) throw new Error(out.error);
  return { url, placeholder: url === PLACEHOLDER, wanted: [], ...out };
}

const byId = (id: unknown) => {
  const p = installed.find((x) => x.manifest.id === id);
  if (!p) throw new Error(`plugin \`${String(id)}\` is not installed`);
  return p;
};

const isEnabled = (id: string) => mockPluginRegistry().find((p) => p.id === id)?.enabled ?? false;

registerMock("plugins_installed", async () => {
  await ready;
  return installed.map((p) => ({ ...structuredClone(p), enabled: isEnabled(p.manifest.id) }));
});
registerMock("plugin_inspect", async ({ path }): Promise<Inspection> => {
  await ready;
  const m = manifest("1.1.0");
  const problems = callWasmSync<unknown[] | { error: string }>((await forgeWasm()).plugins_validate_manifest, {
    manifest: JSON.stringify(m),
    forgeVersion: FORGE_VERSION,
  });
  if (!Array.isArray(problems) || problems.length) throw new Error(`invalid plugin manifest in ${String(path)}`);
  return { manifest: m, sha256: "1".repeat(64), permissionLines: await lines(m.permissions), newPermissionLines: [], installedVersion: "1.0.0" };
});
registerMock("plugin_install_file", async ({ path }) => {
  const p = await install(manifest("1.1.0"), wordCountBundle, { kind: "disk", file: String(path) });
  emitMock(CHANGED, null);
  return p;
});
registerMock("plugin_uninstall", ({ id }) => {
  const p = byId(id);
  installed.splice(installed.indexOf(p), 1);
  const registry = mockPluginRegistry();
  registry.splice(registry.findIndex((r) => r.id === id), 1);
  emitMock(CHANGED, null);
});
registerMock("plugin_bundle", ({ id }) => {
  byId(id);
  if (!isEnabled(String(id))) throw new Error(`plugin \`${String(id)}\` is disabled`);
  return bundles.get(String(id)) ?? "";
});
registerMock("marketplace_list", ({ query }) => marketplace(String(query ?? "")));
registerMock("marketplace_updates", async () => (await marketplace("")).entries.filter((e) => e.updateAvailable));
registerMock("marketplace_install", async ({ id }) => {
  if (id !== "example.word-count") throw new Error("Browser mode cannot download plugins; only the Word Count example can be updated here.");
  const p = await install(manifest("1.1.0"), wordCountBundle, { kind: "marketplace", index: PLACEHOLDER });
  emitMock(CHANGED, null);
  return p;
});
registerMock("plugins_wanted_dismiss", () => undefined);

// The host calls: the example only reads the editor (checked in the UI) and its own settings.
const own = (pluginId: unknown, key: unknown) => {
  if (!String(key).startsWith(`${String(pluginId)}.`)) throw new Error(`plugin \`${String(pluginId)}\` has no permission to access the setting \`${String(key)}\``);
};
registerMock("plugin_host_setting_get", ({ pluginId, key }) => {
  own(pluginId, key);
  return mockSettingValue(String(key));
});
registerMock("plugin_host_setting_set", ({ pluginId, key, value }) => {
  own(pluginId, key);
  setSetting(String(key), value as never, "user");
});
const denied = (what: string) => ({ pluginId }: Record<string, unknown>) => {
  throw new Error(`plugin \`${String(pluginId)}\` has no permission to ${what} (browser mode)`);
};
registerMock("plugin_host_read_file", denied("read files"));
registerMock("plugin_host_write_file", denied("write files"));
registerMock("plugin_host_fetch", ({ pluginId, request }) => denied(`connect to ${(request as FetchRequest).url}`)({ pluginId }));
registerMock("plugin_host_run", denied("start programs"));

// ---- Settings Sync

let lastReport: SyncReport | null = null;
registerMock("settings_sync_status", () => ({
  enabled: !!mockSettingValue("settingsSync.enabled"),
  method: mockSettingValue("settingsSync.method") ?? "folder",
  target: String(mockSettingValue("settingsSync.folder") ?? ""),
  lastReport,
  groups: [
    ["appearance", "Appearance & Behavior"],
    ["keymap", "Keymap"],
    ["editor", "Editor"],
    ["colorSchemes", "Color Schemes"],
    ["codeStyle", "Code Style"],
    ["fileTypes", "File Types"],
    ["templates", "File and Code Templates"],
    ["liveTemplates", "Live Templates"],
    ["plugins", "Plugins"],
    ["installedPlugins", "Installed Plugins (list only)"],
  ],
}));
registerMock("settings_sync_now", () => {
  const folder = String(mockSettingValue("settingsSync.folder") ?? "");
  if (mockSettingValue("settingsSync.method") !== "git" && !folder) throw new Error("choose a sync folder in Settings | Settings Sync");
  lastReport = { pushed: ["keymap", "editor"], pulled: ["colorSchemes"], conflicts: [], atMs: Date.now() };
  return { ...lastReport, restartNeeded: false };
});
