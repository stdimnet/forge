/**
 * Browser-mode third-party plugins and Settings Sync. The SDK's Word Count example
 * (`sdk/examples/word-count`) is installed and runs in the real sandboxed host, and its
 * Shapes Icons example (`sdk/examples/shapes-icons`) contributes an icon theme; Install
 * Plugin from Disk offers its 1.1.0 build, and the demo marketplace index (a copy of Forge's
 * marketplace, `stdimnet/marketplace`, plus the Word Count example) is evaluated by the same
 * Rust code as WebAssembly (`forge-plugins`). Any other index URL fails like an unreachable
 * one, to show the error state. Nothing is downloaded or written, and the access token is
 * only remembered in memory.
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
import shapesManifest from "../../../../sdk/examples/shapes-icons/forge-plugin.json";
import { mockPluginRegistry, mockSettingValue, setSetting } from "./settingsMock";

const FORGE_VERSION = "0.1.0";
const CHANGED = "plugins://changed";
/** `forge_plugins::marketplace::DEFAULT_URL`. */
const DEFAULT_URL = "https://raw.githubusercontent.com/stdimnet/marketplace/main/index.json";
const SETTINGS_PATH = "Settings | Plugins, gear menu | Marketplace Settings";
/** Browser mode keeps the access token in memory only (no keychain). */
const tokens = new Map<string, string>();

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

const ready = Promise.all([
  install(manifest(), wordCountBundle, { kind: "disk", file: "~/Downloads/example.word-count-1.0.0.forgeplugin" }),
  // As Rust's serde defaults: no permissions, no activation events.
  install({ activationEvents: [], permissions: {}, ...structuredClone(shapesManifest) } as unknown as ThirdPartyManifest, "", { kind: "disk", file: "~/Downloads/example.shapes-icons-1.0.0.forgeplugin" }),
]);

// The Shapes Icons package: its files by package path, read by the same Rust loader as an
// installed plugin's (`forge_plugins::icon_themes`, as WebAssembly).
const SHAPES_ROOT = "../../../../sdk/examples/shapes-icons/";
const shapesFiles: Record<string, string> = Object.fromEntries(
  Object.entries(import.meta.glob("../../../../sdk/examples/shapes-icons/theme/**/*", { query: "?raw", import: "default", eager: true })).map(
    ([path, text]) => [path.slice(SHAPES_ROOT.length), text as string],
  ),
);

const MARKET = "https://raw.githubusercontent.com/stdimnet/marketplace/main/";

/** The demo marketplace: Forge's marketplace index as published, plus the SDK example. */
function demoIndex(): string {
  return JSON.stringify({
    formatVersion: 1,
    name: "Forge Marketplace (stdimnet)",
    plugins: [
      {
        id: "reeperk.atom-material-icons",
        name: "Atom Material Icons",
        publisher: "Reeperk",
        description:
          "Material file and folder icons in the style of Atom Material Icons: hundreds of languages, frameworks, tools and special folders, with dark and light variants. Artwork from Material Icon Theme (MIT). Not affiliated with the Atom Material Icons project.",
        homepage: "https://github.com/stdimnet/marketplace/tree/main/plugins/atom-material-icons",
        license: "MIT",
        tags: ["icons", "icon theme", "file icons", "material", "theme"],
        versions: [
          {
            version: "1.0.0",
            engines: { forge: ">=0.1.0" },
            url: `${MARKET}packages/reeperk.atom-material-icons/reeperk.atom-material-icons-1.0.0.forgeplugin`,
            size: 303091,
            sha256: "d94d4df2722182ac35359e718b52f01f161c8620dc9f1537c2b25de2ff046647",
            permissions: {},
            released: "2026-11-05",
            changes:
              "First release: 469 Material icons for files and folders (dark and light UI variants), from Material Icon Theme at commit f368264. Laravel, Composer, PHPUnit, Node.js, Vite, Docker, Git, GitHub Actions and many more tools, over 150 file types and over 100 special folders, each with an open variant. Chevron tree arrows and 16 px icons by default; accent color, monochrome, arrows and size can be changed in Settings | Appearance & Behavior | File Icons.",
          },
        ],
      },
      {
        id: "example.word-count",
        name: "Word Count",
        publisher: "Forge SDK Examples",
        description: wordCountManifest.description,
        tags: ["editor", "writing"],
        versions: ["1.0.0", "1.1.0"].map((version) => ({
          version,
          engines: { forge: ">=0.1.0" },
          url: `${MARKET}packages/example.word-count/example.word-count-${version}.forgeplugin`,
          sha256: "9f2c4e5a1b7d3c8e6f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c0d2e",
          size: 4211,
          permissions: { editor: ["read"] },
          released: version === "1.0.0" ? "2026-10-28" : "2026-11-02",
          ...(version === "1.1.0" ? { changes: "Reading time in the tool window." } : {}),
        })),
      },
    ],
  });
}

const indexUrl = () => (mockSettingValue("plugins.marketplace.url") as string | null)?.trim() || DEFAULT_URL;

async function marketplace(query: string) {
  await ready;
  const url = indexUrl();
  if (url !== DEFAULT_URL) {
    // As `forge_plugins::marketplace::http_error` for a 404 without a token.
    const hint = tokens.has(tokenScope(url))
      ? `Check the index URL in ${SETTINGS_PATH}, and that the access token may read the repository.`
      : `Check the index URL in ${SETTINGS_PATH}; a private repository (on GitHub, for example) also answers 404 until you set an access token there.`;
    throw new Error(`${url}: HTTP 404 Not Found. ${hint}`);
  }
  const out = callWasmSync<{ name: string; entries: MarketplaceEntry[]; warnings: string[] } | { error: string }>(
    (await forgeWasm()).plugins_marketplace,
    { index: demoIndex(), forgeVersion: FORGE_VERSION, installed: installed.map((p) => ({ id: p.manifest.id, version: p.manifest.version })), query },
  );
  if ("error" in out) throw new Error(out.error);
  return { url, wanted: [], ...out };
}

/** `forge_plugins::marketplace::token_scope`. */
function tokenScope(url: string): string {
  try {
    const u = new URL(url);
    return ["raw.githubusercontent.com", "github.com", "api.github.com"].includes(u.hostname) ? "github.com" : u.host;
  } catch {
    return "";
  }
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
  const p = await install(manifest("1.1.0"), wordCountBundle, { kind: "marketplace", index: indexUrl() });
  emitMock(CHANGED, null);
  return p;
});
registerMock("marketplace_token_status", () => {
  const url = indexUrl();
  return { url, scope: tokenScope(url), set: tokens.has(tokenScope(url)) };
});
registerMock("marketplace_token_set", ({ token }) => {
  const scope = tokenScope(indexUrl());
  const t = typeof token === "string" ? token.trim() : "";
  if (t) tokens.set(scope, t);
  else tokens.delete(scope);
});
registerMock("plugins_wanted_dismiss", () => undefined);
registerMock("icon_theme_load", async ({ id }) => {
  await ready;
  const owner = installed.find((p) => (p.manifest.contributes.iconThemes ?? []).some((t) => t.id === id));
  if (!owner) throw new Error(`no installed plugin contributes the icon theme \`${String(id)}\``);
  if (!isEnabled(owner.manifest.id)) throw new Error(`plugin \`${owner.manifest.id}\` is disabled`);
  const files = owner.manifest.id === shapesManifest.id ? shapesFiles : {};
  const out = callWasmSync<object | { error: string }>((await forgeWasm()).plugins_load_icon_theme, {
    manifest: JSON.stringify(owner.manifest),
    theme: id,
    files,
  });
  if ("error" in out) throw new Error(out.error);
  return out;
});

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
