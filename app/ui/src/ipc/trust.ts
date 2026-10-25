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
}

/** An entry of Settings | Build, Execution, Deployment | Trusted Locations. */
export interface TrustedEntry {
  key: string;
  display: string;
  kind: "location" | "project";
}

export const TRUST_CHANGED_EVENT = "trust://changed";

export const trustApi = {
  /** Whether a local folder is trusted, before it is opened. */
  check: (path: string) => invoke<TrustCheck>("trust_check", { path }),
  status: () => invoke<TrustStatus | null>("trust_status"),
  /** Trust `path` (or the open project without it); `location` trusts its whole folder. */
  trust: (path?: string, location?: string) => invoke<TrustCheck>("trust_project", { path, location }),
  locations: () => invoke<TrustedEntry[]>("trust_locations"),
  addLocation: (path: string) => invoke<TrustedEntry[]>("trust_location_add", { path }),
  removeLocation: (key: string) => invoke<TrustedEntry[]>("trust_location_remove", { key }),
  onChanged: (cb: (s: TrustStatus | null) => void) => listen<TrustStatus | null>(TRUST_CHANGED_EVENT, cb),
};
