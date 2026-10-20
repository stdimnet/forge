/**
 * Language servers for many languages (see app/src-tauri/src/commands/langservers.rs):
 * the Language Servers settings page, installs with streamed output, project-wide symbol
 * search over every running server and the Intelephense licence key (OS keychain).
 */
import { invoke, listen } from "./index";
import type { LspRange } from "./lsp";

export type RowStatus = "running" | "installed" | "notInstalled" | "disabled";

export type SkipReason =
  | { reason: "autoStartOff" }
  | { reason: "disabled" }
  | { reason: "pluginDisabled"; plugin: string }
  | { reason: "notInProject" }
  | { reason: "notUsed" }
  | { reason: "notInstalled" }
  | { reason: "alternative"; used: string };

export interface InstallRow {
  /** Index into the server's install options (what `install` takes). */
  index: number;
  /** npm, pip, pipx, go, cargo, rustup, gem, dotnet, composer, brew, pacman, apt, download */
  tool: string;
  /** The command line, or the download page. */
  command: string;
  /** Forge may run it (no root or browser needed). */
  runnable: boolean;
  /** The tool was found on this machine. */
  available: boolean;
  url?: string | null;
}

export interface ServerRow {
  id: string;
  name: string;
  /** Heading on the settings page, e.g. "JavaScript & TypeScript". */
  group: string;
  languages: string[];
  command: string;
  args: string[];
  homepage: string;
  license: string;
  note?: string | null;
  enabled: boolean;
  defaultEnabled: boolean;
  status: RowStatus;
  /** "Indexing 45%", a start error, ... */
  detail?: string | null;
  /** Detected or custom executable. */
  path?: string | null;
  customPath?: string | null;
  /** One of its languages occurs in the open project. */
  inProject: boolean;
  skip?: SkipReason | null;
  install: InstallRow[];
  /** Built-in `{ initializationOptions, settings }`. */
  defaults: { initializationOptions: unknown; settings: unknown };
  extraSettings?: string | null;
  extraSettingsError?: string | null;
  /** "catalog", or the pack that starts it itself ("php"). */
  source: string;
}

export type InstallStream = "stdout" | "stderr" | "info";

export interface InstallLine {
  id: string;
  stream: InstallStream;
  line: string;
}

export type InstallOutcome = "succeeded" | "cancelled" | { failed: { code?: number | null } };

export interface WorkspaceSymbol {
  name: string;
  /** "class", "interface", "function", "method", "property", "variable", ... */
  kind: string;
  containerName?: string | null;
  /** Workspace relative. */
  path: string;
  range?: LspRange | null;
  /** Id of the server that found it. */
  server: string;
}

export interface LicenceStatus {
  premium: boolean;
  /** "…1234" */
  maskedKey?: string | null;
  error?: string | null;
  purchaseUrl: string;
}

/** Settings keys of per-server preferences. */
export const serverKey = (id: string, field: "enabled" | "path" | "settings") => `langservers.servers.${id}.${field}`;

export const langserversApi = {
  list: () => invoke<ServerRow[]>("langservers_list"),
  /** Re-plan after settings changed; returns the new rows. */
  apply: () => invoke<ServerRow[]>("langservers_apply"),
  install: (id: string, index: number) => invoke<InstallOutcome>("langservers_install", { id, index }),
  cancelInstall: (id: string) => invoke<void>("langservers_cancel_install", { id }),
  workspaceSymbols: (query: string, limit = 200) => invoke<WorkspaceSymbol[]>("lsp_workspace_symbols", { query, limit }),
  licenceStatus: () => invoke<LicenceStatus>("intelephense_licence_status"),
  setLicence: (key: string | null) => invoke<LicenceStatus>("intelephense_set_licence", { key }),
  onInstallOutput: (cb: (l: InstallLine) => void) => listen<InstallLine>("langservers://install-output", cb),
  onChanged: (cb: () => void) => listen<unknown>("langservers://changed", () => cb()),
};
