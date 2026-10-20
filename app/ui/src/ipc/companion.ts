/**
 * Browser Companion and other external open requests (`forge://` links, `forge file:12`).
 * Rust side: `app/src-tauri/src/commands/companion.rs` and `crates/forge-open`.
 */
import { invoke, listen } from "./index";

export interface ServerStatus {
  running: boolean;
  port?: number | null;
  error?: string | null;
}

export interface PairingCode {
  /** Six digits. */
  code: string;
  /** Unix seconds. */
  expiresAt: number;
}

export interface PairedClient {
  id: string;
  name: string;
  origin?: string | null;
  createdAt: number;
  lastUsedAt?: number | null;
}

export interface CompanionStatus {
  server: ServerStatus;
  pairing: PairingCode | null;
  clients: PairedClient[];
}

/** Where a requested file lives, relative to `root`. */
export type OpenTarget =
  | { kind: "open"; root: string; path: string }
  | { kind: "recent"; root: string; name: string; path: string }
  | { kind: "outside"; suggestedRoot: string; path: string };

export type OpenSource = "browser" | "link" | "cli";

export interface OpenEvent {
  source: OpenSource;
  /** Paired browser name for `source: "browser"`. */
  client?: string | null;
  /** Absolute path. */
  file: string;
  line?: number | null;
  column?: number | null;
  target: OpenTarget;
}

export type PendingEvent = ({ type: "open" } & OpenEvent) | { type: "openProject"; root: string };

export interface ProjectProfile {
  composer: string[];
  npm: string[];
  /** Dotenv file → requested keys that are set. */
  env: Record<string, Record<string, string>>;
  /** Probed files that exist. */
  files: string[];
}

export const COMPANION_OPEN_EVENT = "companion://open";
export const COMPANION_OPEN_PROJECT_EVENT = "companion://open-project";
export const COMPANION_STATUS_EVENT = "companion://status";

export const companionApi = {
  status: () => invoke<CompanionStatus>("companion_status"),
  startPairing: () => invoke<PairingCode>("companion_start_pairing"),
  cancelPairing: () => invoke<void>("companion_cancel_pairing"),
  revokeClient: (id: string) => invoke<boolean>("companion_revoke_client", { id }),
  /** Subscribe first, then call this to receive requests that arrived before the UI. */
  uiReady: () => invoke<PendingEvent[]>("companion_ui_ready"),
  openLink: (url: string) => invoke<void>("companion_open_link", { url }),
  projectProfile: (envKeys: string[], files: string[]) =>
    invoke<ProjectProfile>("companion_project_profile", { envKeys, files }),
  setEnvVar: (file: ".env" | ".env.local", key: string, value: string) =>
    invoke<void>("companion_set_env_var", { file, key, value }),
  onOpen: (cb: (e: OpenEvent) => void) => listen<OpenEvent>(COMPANION_OPEN_EVENT, cb),
  onOpenProject: (cb: (root: string) => void) => listen<string>(COMPANION_OPEN_PROJECT_EVENT, cb),
  onStatus: (cb: (s: ServerStatus) => void) => listen<ServerStatus>(COMPANION_STATUS_EVENT, cb),
};
