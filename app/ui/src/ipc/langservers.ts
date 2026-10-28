/**
 * Language servers for many languages (see app/src-tauri/src/commands/langservers.rs):
 * the Language Servers settings page, installs with streamed output, project-wide symbol
 * search over every running server.
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
  /**
   * Forge installs it itself, without prerequisites: npm packages are downloaded from the
   * registry (sha512 checked) into Forge's directory, with Node.js when none is installed.
   */
  managed?: boolean;
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
  /** Version of a server built into Forge (php-lsp), e.g. "0.1.0 (6e00c57)". */
  builtinVersion?: string | null;
}

export type InstallStream = "stdout" | "stderr" | "info";

export interface InstallLine {
  id: string;
  stream: InstallStream;
  line: string;
}

export type InstallOutcome = "succeeded" | "cancelled" | { failed: { code?: number | null } };

/** A download of a managed install (Node.js, a package tarball). */
export interface InstallProgress {
  /** Server id (or package name for updates). */
  id: string;
  /** "Downloading Node.js v22.11.0" */
  label: string;
  done: number;
  total?: number | null;
}

/** A newer version of an npm server Forge installed. */
export interface PackageUpdate {
  name: string;
  installed: string;
  latest: string;
}

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

/** Settings keys of per-server preferences. */
export const serverKey = (id: string, field: "enabled" | "path" | "settings") => `langservers.servers.${id}.${field}`;

export const langserversApi = {
  list: () => invoke<ServerRow[]>("langservers_list"),
  /** Re-plan after settings changed; returns the new rows. */
  apply: () => invoke<ServerRow[]>("langservers_apply"),
  install: (id: string, index: number) => invoke<InstallOutcome>("langservers_install", { id, index }),
  cancelInstall: (id: string) => invoke<void>("langservers_cancel_install", { id }),
  /** Newer versions of Forge-installed servers; null when not checked (once a day). */
  checkUpdates: (force = false) => invoke<PackageUpdate[] | null>("langservers_check_updates", { force }),
  /** Update a Forge-installed npm package to its latest version. */
  update: (pkg: string) => invoke<InstallOutcome>("langservers_update", { package: pkg }),
  onInstallProgress: (cb: (p: InstallProgress) => void) => listen<InstallProgress>("langservers://install-progress", cb),
  workspaceSymbols: (query: string, limit = 200) => invoke<WorkspaceSymbol[]>("lsp_workspace_symbols", { query, limit }),
  onInstallOutput: (cb: (l: InstallLine) => void) => listen<InstallLine>("langservers://install-output", cb),
  onChanged: (cb: () => void) => listen<unknown>("langservers://changed", () => cb()),
};
