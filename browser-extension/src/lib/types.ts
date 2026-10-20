/** Shapes shared by the page script, content script, background worker and popup. */

export type Framework = "react" | "vue" | "vue2" | "angular" | "svelte" | "attribute";

/** Where a component (or element) under the cursor was written, as far as the page knows. */
export interface SourceInfo {
  framework: Framework;
  /** Component name, e.g. `ProductCard` or `UserCardComponent`. */
  name?: string;
  /** Raw source reference: absolute path, project-relative path, `webpack://…` or a dev-server URL. */
  file?: string;
  /** 1-based. */
  line?: number;
  /** 1-based. */
  column?: number;
}

export interface ForgeProject {
  name: string;
  root: string;
}

export interface PingResult {
  name: string;
  version: string;
  api: number;
  paired: boolean;
  client?: string;
  projects?: ForgeProject[];
}

/** A source location to open, after mapping it to Forge's file system. */
export interface OpenTarget {
  file: string;
  line?: number;
  column?: number;
  /** Project root the file is relative to (when `file` is relative). */
  project?: string;
  /** Component name used to find the declaration when there is no line. */
  symbol?: string;
}

export interface PathMapping {
  /** Path prefix as the page reports it (e.g. inside a Docker container). */
  from: string;
  /** Local path prefix. */
  to: string;
}

export interface RecentEntry {
  label: string;
  file: string;
  /** `file` relative to its project, for display. */
  path?: string;
  line?: number;
  column?: number;
  /** Unix milliseconds. */
  at: number;
}

export interface Settings {
  /** Port Forge answered on last time (scanned again if it stops answering). */
  port?: number;
  token?: string;
  altClick: boolean;
  mappings: PathMapping[];
  recent: RecentEntry[];
}

export const DEFAULT_SETTINGS: Settings = { altClick: true, mappings: [], recent: [] };

/** Messages to the background worker. */
export type BackgroundRequest =
  | { type: "status" }
  | { type: "pair"; code: string }
  | { type: "unpair" }
  | { type: "open"; source: SourceInfo }
  | { type: "openText"; text: string }
  | { type: "openRecent"; entry: RecentEntry }
  | { type: "setInspect"; tabId?: number; enabled: boolean }
  | { type: "getInspect"; tabId?: number }
  | { type: "contentConfig" }
  | { type: "saveOptions"; altClick?: boolean; mappings?: PathMapping[]; port?: number | null };

export interface Status {
  connected: boolean;
  port?: number;
  ping?: PingResult;
  error?: string;
  inspecting: boolean;
  settings: Settings;
}

export interface OpenResult {
  ok: boolean;
  /** "opened" | "needsConfirmation" from Forge. */
  status?: string;
  label?: string;
  error?: string;
}

/** Messages to the content script. */
export type ContentMessage =
  | { type: "inspect"; enabled: boolean }
  | { type: "openContextTarget"; linkUrl?: string }
  | { type: "toast"; text: string; error?: boolean };

export interface ContentConfig {
  altClick: boolean;
  roots: string[];
  mappings: PathMapping[];
  inspecting: boolean;
  /** Shortcut of the toggle-inspect command, as configured in the browser. */
  shortcut?: string;
}
