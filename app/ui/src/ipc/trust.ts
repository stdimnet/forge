/** Project trust and Safe Mode (`forge-trust`, `commands/trust.rs`). */
import { invoke, listen } from "./index";

export type TrustReason = { kind: "project" } | { kind: "location"; location: string };

/** A project's trust state, for "Trust and Open Project?" and the Safe Mode banner. */
export interface TrustCheck {
  /** The project's key: a local path, or `ssh://user@host:port/path`. */
  project: string;
  /** The project as shown (`user@host:/path` for remote projects). */
  display: string;
  trusted: boolean;
  reason?: TrustReason;
  /** The folder containing the project, for "Trust projects in …" (absent for a root). */
  parent?: string;
  parentDisplay?: string;
}

/** The open project's trust state. */
export interface TrustStatus extends TrustCheck {
  /** Nothing of the project runs. */
  safeMode: boolean;
  remote: boolean;
  /**
   * The host's project of a Code With Me session: always in Safe Mode on this machine and
   * not trusted from here, so no banner and no Trust Project… (the host runs things).
   */
  guest: boolean;
}

/** An entry of Settings | Build, Execution, Deployment | Trusted Locations. */
export interface TrustedEntry {
  key: string;
  display: string;
  kind: "location" | "project";
}

/** "Trust and Open Project?" for the project a launch opened in this window. */
export interface LaunchTrustPrompt {
  status: TrustStatus;
  /** The window was opened for the project (New Window): Don't Open closes it. */
  closeWindowOnCancel: boolean;
}

export const TRUST_CHANGED_EVENT = "trust://changed";

export const trustApi = {
  /** Whether a local folder is trusted, before it is opened. */
  check: (path: string) => invoke<TrustCheck>("trust_check", { path }),
  status: () => invoke<TrustStatus | null>("trust_status"),
  /** Trust `path` (or the open project without it); `location` trusts its whole folder. */
  trust: (path?: string, location?: string) => invoke<TrustCheck>("trust_project", { path, location }),
  /**
   * The project the command line, "Open Folder in Forge" or Open Project › New Window opened
   * in this window, when it is untrusted: asked once, here, in the window it opened in.
   */
  launchPrompt: () => invoke<LaunchTrustPrompt | null>("trust_launch_prompt"),
  locations: () => invoke<TrustedEntry[]>("trust_locations"),
  addLocation: (path: string) => invoke<TrustedEntry[]>("trust_location_add", { path }),
  removeLocation: (key: string) => invoke<TrustedEntry[]>("trust_location_remove", { key }),
  onChanged: (cb: (s: TrustStatus | null) => void) => listen<TrustStatus | null>(TRUST_CHANGED_EVENT, cb),
};
