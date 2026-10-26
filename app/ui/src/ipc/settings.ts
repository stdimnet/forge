/**
 * Settings and plugin registry IPC. Mirrors `forge_plugin_api` contribution types and the
 * `settings_*` / `plugin*` commands in `app/src-tauri/src/commands/settings.rs`.
 */
import { invoke } from "./index";
import type { MainMenuContribution } from "./keymap";

/** An entry of an `objects` setting: a table of string values. */
export type SettingObject = Record<string, string>;

export type SettingValue = boolean | number | string | string[] | SettingObject[];
/** `newProjects`: Settings for New Projects (defaults of project-level settings). */
export type SettingsScope = "user" | "newProjects" | "project";

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
  | { type: "list" }
  /** A list of tables; no two entries may share the values of all `unique` fields. */
  | { type: "objects"; unique?: string[] };

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
  /** Edited by a dialog of its page instead of a row. */
  hidden?: boolean;
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
  /** Interpreters of `#!` lines that select the type. */
  interpreters?: string[];
  indentSize?: number;
  /** Template language (text between tags uses a data language). */
  template?: boolean;
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
  /** Test frameworks for the shared test runner (each is also a run configuration type). */
  testFrameworks?: import("./testing").TestFrameworkSpec[];
  aiProviders: AiProviderContribution[];
  chatAgents: ChatAgentContribution[];
  codeGenerators: CodeGeneratorContribution[];
  /** Main menu entries (`[[contributions.mainMenu]]`). */
  mainMenu?: MainMenuContribution[];
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
  /** Effective values (defaults < user < settings for new projects < project). */
  values: Record<string, SettingValue>;
  user: Record<string, SettingValue>;
  project: Record<string, SettingValue>;
  /** Settings for New Projects. */
  newProjects?: Record<string, SettingValue>;
  hasProject: boolean;
}

/** Payload of the `settings://changed` event. */
export interface SettingsChange {
  key: string;
  value: SettingValue | null;
  scope: "default" | SettingsScope;
}

export const SETTINGS_CHANGED_EVENT = "settings://changed";

/** A settings page as sent to the Settings dialog model (`settings_search`). */
export interface SettingsPageInput {
  id: string;
  title: string;
  group: string;
  parent?: string;
  keywords?: string[];
  /** Searchable text beyond title, keywords and the page's schema settings. */
  text?: string;
}

export interface SettingsTreeNode {
  /** Page id, or `group:<name>` for a group without a page of its own. */
  id: string;
  title: string;
  /** Whether `id` is a page. */
  page: boolean;
  /** Page ids in this subtree, including the node's own. */
  pages: string[];
  children: SettingsTreeNode[];
}

export interface HighlightSegment {
  text: string;
  hit: boolean;
}

export interface SettingsSearchRequest {
  query: string;
  pages?: SettingsPageInput[];
  settings?: SettingSpec[];
  entries?: { id: string; text: string }[];
  highlight?: string[];
}

export interface SettingsSearchResponse {
  tree: SettingsTreeNode[];
  /** Matching nodes and their ancestors (the whole tree for an empty query). */
  visible: SettingsTreeNode[];
  /** Nodes that match themselves. */
  matches: string[];
  firstMatch: string | null;
  /** Node ids from the root to each node. */
  breadcrumbs: Record<string, string[]>;
  /** Keys of matching settings. */
  settings: string[];
  /** Ids of matching entries. */
  entries: string[];
  /** Segments of each requested highlight text, in order. */
  highlights: HighlightSegment[][];
}

export const settingsApi = {
  getAll: () => invoke<SettingsSnapshot>("settings_get_all"),
  /** `null` removes the override in `scope` so the value is inherited again. */
  set: (key: string, value: SettingValue | null, scope: SettingsScope) =>
    invoke<void>("settings_set", { key, value, scope }),
  schema: () => invoke<SettingSpec[]>("settings_schema"),
  plugins: () => invoke<PluginInfo[]>("plugins_list"),
  /** Search the Settings dialog (page tree, matches, highlights). */
  search: (request: SettingsSearchRequest) => invoke<SettingsSearchResponse>("settings_search", { request }),
  /** The problem with a value for `key`, or null when it is valid. */
  validate: (key: string, value: SettingValue) => invoke<string | null>("settings_validate", { key, value }),
  /** Returns the ids whose state changed (dependencies/dependents cascade). */
  setPluginEnabled: (id: string, enabled: boolean) =>
    invoke<string[]>("plugin_set_enabled", { id, enabled }),
};
