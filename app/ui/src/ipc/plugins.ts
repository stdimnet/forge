/**
 * Third-party plugins (`crates/forge-plugins`, `commands/third_party.rs`): Install Plugin
 * from Disk, uninstall, the plugin host's permission-checked backend calls and the
 * Marketplace tab. Settings Sync lives in `commands/settings_sync.rs`.
 */
import type { IconThemeDef } from "@/features/iconThemes/state";
import { invoke } from "./index";
import type { CommandContribution, KeybindingContribution, SettingSpec, ToolWindowContribution } from "./settings";

export type EditorAccess = "read" | "write";

export interface PluginPermissions {
  fs?: { read?: string[]; write?: string[] };
  network?: string[];
  process?: string[];
  settings?: string[];
  editor?: EditorAccess[];
}

export interface StatusItemContribution {
  id: string;
  alignment: "left" | "right";
  order: number;
  command?: string;
}

export interface FileTypeContribution {
  id: string;
  name: string;
  extensions: string[];
  fileNames: string[];
}

/** `forge-plugin.json` (manifest format 1). */
export interface ThirdPartyManifest {
  manifestVersion: number;
  id: string;
  name: string;
  version: string;
  publisher: string;
  description: string;
  license?: string;
  homepage?: string;
  engines: { forge: string };
  main?: string;
  activationEvents: string[];
  permissions: PluginPermissions;
  contributes: {
    commands?: CommandContribution[];
    keybindings?: KeybindingContribution[];
    statusItems?: StatusItemContribution[];
    toolWindows?: ToolWindowContribution[];
    settingsPages?: { id: string; title: string }[];
    settings?: SettingSpec[];
    languages?: FileTypeContribution[];
    /** Icon themes: `{ id, label, path }` of an `icon-theme.json` in the package. */
    iconThemes?: { id: string; label: string; path: string }[];
  };
}

/** One line of the permission prompt (computed in Rust, most dangerous first). */
export interface PermissionLine {
  kind: string;
  label: string;
  detail: string;
  risk: "low" | "medium" | "high";
}

export type PluginSource = { kind: "disk"; file: string } | { kind: "marketplace"; index: string };

export interface InstalledPlugin {
  manifest: ThirdPartyManifest;
  enabled: boolean;
  granted: PluginPermissions;
  source: PluginSource;
  sha256: string;
  installedAt: number;
  permissionLines: PermissionLine[];
  /** Not loaded in Safe Mode (starts programs or writes project files). */
  needsTrust: boolean;
}

export interface Inspection {
  manifest: ThirdPartyManifest;
  sha256: string;
  permissionLines: PermissionLine[];
  newPermissionLines: PermissionLine[];
  installedVersion?: string | null;
}

export interface IndexVersion {
  version: string;
  engines: { forge: string };
  url: string;
  sha256: string;
  size: number;
  signature?: string;
  permissions: PluginPermissions;
  released?: string;
  changes?: string;
}

export interface MarketplaceEntry {
  id: string;
  name: string;
  publisher: string;
  description: string;
  homepage?: string;
  license?: string;
  tags: string[];
  downloads: number;
  latest: IndexVersion;
  compatible: boolean;
  installedVersion?: string;
  updateAvailable: boolean;
  permissionLines: PermissionLine[];
}

export interface PluginRef {
  id: string;
  version: string;
  name: string;
  enabled: boolean;
  marketplace: boolean;
}

export interface MarketplaceView {
  url: string;
  placeholder: boolean;
  name: string;
  entries: MarketplaceEntry[];
  warnings: string[];
  wanted: PluginRef[];
}

export interface FetchRequest {
  url: string;
  method?: string;
  headers?: Record<string, string>;
  body?: string;
}

export interface FetchResponse {
  status: number;
  headers: Record<string, string>;
  body: string;
}

export interface RunOutput {
  exitCode: number | null;
  stdout: string;
  stderr: string;
}

export interface SyncConflict {
  group: string;
  winner: "local" | "remote";
  backup: string;
}

export interface SyncReport {
  pushed: string[];
  pulled: string[];
  conflicts: SyncConflict[];
  atMs: number;
}

export interface SyncStatus {
  enabled: boolean;
  method: "folder" | "git";
  target: string;
  lastReport?: SyncReport | null;
  groups: [string, string][];
}

export type SyncResult = SyncReport & { restartNeeded: boolean };

export const pluginsApi = {
  installed: () => invoke<InstalledPlugin[]>("plugins_installed"),
  inspect: (path: string) => invoke<Inspection>("plugin_inspect", { path }),
  installFile: (path: string, sha256: string) => invoke<InstalledPlugin>("plugin_install_file", { path, sha256 }),
  uninstall: (id: string) => invoke<void>("plugin_uninstall", { id }),
  bundle: (id: string) => invoke<string>("plugin_bundle", { id }),
  /** An icon theme of an enabled plugin with its SVGs (`forge_plugins::icon_themes`). */
  iconTheme: (id: string) => invoke<LoadedIconTheme>("icon_theme_load", { id }),
  marketplace: (query: string, refresh = false) => invoke<MarketplaceView>("marketplace_list", { query, refresh }),
  marketplaceInstall: (id: string, version?: string) =>
    invoke<InstalledPlugin>("marketplace_install", { id, version: version ?? null }),
  updates: () => invoke<MarketplaceEntry[]>("marketplace_updates"),
  dismissWanted: () => invoke<void>("plugins_wanted_dismiss"),
};

/** Backend calls of the plugin host; Rust checks every one against the granted permissions. */
export const pluginHostApi = {
  readFile: (pluginId: string, path: string) => invoke<string>("plugin_host_read_file", { pluginId, path }),
  writeFile: (pluginId: string, path: string, contents: string) =>
    invoke<void>("plugin_host_write_file", { pluginId, path, contents }),
  fetch: (pluginId: string, request: FetchRequest) => invoke<FetchResponse>("plugin_host_fetch", { pluginId, request }),
  run: (pluginId: string, program: string, args: string[]) => invoke<RunOutput>("plugin_host_run", { pluginId, program, args }),
  getSetting: (pluginId: string, key: string) => invoke<unknown>("plugin_host_setting_get", { pluginId, key }),
  setSetting: (pluginId: string, key: string, value: unknown) =>
    invoke<void>("plugin_host_setting_set", { pluginId, key, value }),
};

export const settingsSyncApi = {
  status: () => invoke<SyncStatus>("settings_sync_status"),
  syncNow: () => invoke<SyncResult>("settings_sync_now"),
};

/** `forge_plugins::icon_themes::LoadedIconTheme`. */
export interface LoadedIconTheme {
  id: string;
  label: string;
  pluginId: string;
  definition: IconThemeDef;
  /** Icon id → SVG text. */
  svgs: Record<string, string>;
}
