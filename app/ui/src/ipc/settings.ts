/**
 * Settings and plugin registry IPC. Mirrors `forge_plugin_api` contribution types and the
 * `settings_*` / `plugin*` commands in `app/src-tauri/src/commands/settings.rs`.
 */
import { invoke } from "./index";

export type SettingValue = boolean | number | string | string[];
export type SettingsScope = "user" | "project";

export interface EnumOption {
  value: string;
  label: string;
}

export type SettingType =
  | { type: "bool" }
  | { type: "int"; min?: number; max?: number }
  | { type: "number"; min?: number; max?: number; step?: number }
  | { type: "string" }
  | { type: "enum"; options: EnumOption[] }
  | { type: "path" }
  | { type: "list" };

export type SettingSpec = SettingType & {
  key: string;
  title: string;
  description?: string;
  default: SettingValue;
  /** Settings page id the setting is shown on. */
  group: string;
  section?: string;
  /** Stored in `.forge/settings.toml` of the open project. */
  project: boolean;
  order: number;
};

export interface CommandContribution {
  id: string;
  title: string;
  category?: string;
}

export interface KeybindingContribution {
  command: string;
  key: string;
  when?: string;
  keymap?: string;
}

export interface ToolWindowContribution {
  id: string;
  title: string;
  anchor: "left" | "right" | "bottom";
  icon?: string;
  order: number;
}

export interface LspServerSpec {
  id: string;
  languageId: string;
  command: string;
  args: string[];
  fileExtensions: string[];
  initializationOptions?: unknown;
}

export interface RunConfigurationType {
  id: string;
  name: string;
  description?: string;
  icon?: string;
  debuggable: boolean;
  /** Form fields of manifest-only types (stored in the configuration's `options`). */
  fields?: RunConfigurationFieldSpec[];
  /** Command template, e.g. `["${interpreter}", "${file}", "${arguments}"]`. */
  command?: string[];
  defaultInterpreter?: string;
}

export interface RunConfigurationFieldSpec {
  key: string;
  label: string;
  kind?: "string" | "path" | "text" | "enum" | "bool";
  options?: { value: string; label: string }[];
  default?: string;
  required?: boolean;
}

export interface AiProviderContribution {
  id: string;
  name: string;
  kind: "anthropic" | "openai-compatible" | "gemini" | "cli";
  baseUrl?: string;
  models: string[];
  defaultModel?: string;
  requiresApiKey: boolean;
}

export interface ChatAgentContribution {
  id: string;
  name: string;
  description?: string;
  provider?: string;
}

export interface CodeGeneratorContribution {
  id: string;
  title: string;
  languages: string[];
  description?: string;
}

export interface FileTypeContribution {
  id: string;
  name: string;
  extensions: string[];
  fileNames: string[];
}

export interface Contributions {
  languages: string[];
  fileTypes: FileTypeContribution[];
  commands: CommandContribution[];
  keybindings: KeybindingContribution[];
  toolWindows: ToolWindowContribution[];
  settings: SettingSpec[];
  lspServers: LspServerSpec[];
  runConfigurationTypes: RunConfigurationType[];
  aiProviders: AiProviderContribution[];
  chatAgents: ChatAgentContribution[];
  codeGenerators: CodeGeneratorContribution[];
}

export type PackId = "core" | "phpforge" | "webforge" | "luaforge" | "scriptforge" | "configforge";

export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  pack: PackId | string;
  description: string;
  vendor?: string;
  depends: string[];
  required: boolean;
  enabled: boolean;
  /** Language/framework/pack ids the plugin applies to; missing means always active. */
  projectTypes?: string[];
  contributions: Contributions;
}

export interface SettingsSnapshot {
  /** Effective values (defaults < user < project). */
  values: Record<string, SettingValue>;
  user: Record<string, SettingValue>;
  project: Record<string, SettingValue>;
  hasProject: boolean;
}

/** Payload of the `settings://changed` event. */
export interface SettingsChange {
  key: string;
  value: SettingValue | null;
  scope: "default" | SettingsScope;
}

export const SETTINGS_CHANGED_EVENT = "settings://changed";

export const settingsApi = {
  getAll: () => invoke<SettingsSnapshot>("settings_get_all"),
  /** `null` removes the override in `scope` so the value is inherited again. */
  set: (key: string, value: SettingValue | null, scope: SettingsScope) =>
    invoke<void>("settings_set", { key, value, scope }),
  schema: () => invoke<SettingSpec[]>("settings_schema"),
  plugins: () => invoke<PluginInfo[]>("plugins_list"),
  /** Returns the ids whose state changed (dependencies/dependents cascade). */
  setPluginEnabled: (id: string, enabled: boolean) =>
    invoke<string[]>("plugin_set_enabled", { id, enabled }),
};
