/**
 * Code With Me (`app/src-tauri/src/commands/collab.rs`, `crates/forge-collab`). The host
 * shares the files open in its editor; everyone edits them together. Browser mode answers
 * with `mock/collabMock.ts`.
 */
import { invoke, listen } from "./index";
import type { WorkspaceInfo } from "./workspace";

export type PermissionLevel = "readOnly" | "editFiles" | "fullAccess" | "custom";

export interface Permissions {
  files: "readOnly" | "edit";
  terminal: "none" | "readOnly" | "full";
  run: "none" | "readOnly" | "full";
  vcs: "readOnly" | "full";
}

export const PRESETS: Record<Exclude<PermissionLevel, "custom">, Permissions> = {
  readOnly: { files: "readOnly", terminal: "readOnly", run: "readOnly", vcs: "readOnly" },
  editFiles: { files: "edit", terminal: "none", run: "none", vcs: "readOnly" },
  fullAccess: { files: "edit", terminal: "full", run: "full", vcs: "full" },
};

/** The preset these permissions are, or `custom`. */
export function levelOf(p: Permissions): PermissionLevel {
  for (const [level, preset] of Object.entries(PRESETS)) {
    if (preset.files === p.files && preset.terminal === p.terminal && preset.run === p.run && preset.vcs === p.vcs) {
      return level as PermissionLevel;
    }
  }
  return "custom";
}

export interface Participant {
  id: number;
  name: string;
  role: "host" | "guest";
  /** 1-based index of the theme's `--collab-N` colors. */
  color: number;
  permissions: Permissions;
}

export interface SharedFile {
  path: string;
}

export interface SessionView {
  role: "none" | "host" | "guest";
  /** Start Session can be used; otherwise `hostUnavailable` says why. */
  canHost: boolean;
  hostUnavailable?: string;
  /** The invitation link (host). */
  link?: string;
  me?: number;
  participants: Participant[];
  /** Files guests can open. */
  files: SharedFile[];
  /** The session's guest permissions (host) or this guest's permissions. */
  permissions?: Permissions;
  /** `name (machine)` of the host (guest). */
  hostLabel?: string;
  /** Shown to every participant, e.g. that the session is a development preview. */
  notice?: string;
  /** The host's project, open in this window (guest). */
  workspace?: WorkspaceInfo;
}

/** What the Join Session dialog shows about a pasted link. */
export interface LinkSummary {
  hostName: string;
  hostLabel: string;
  sessionId: string;
  relay?: string;
}

export const STATUS_EVENT = "collab://status";
export const ENDED_EVENT = "collab://ended";

export const collabApi = {
  status: () => invoke<SessionView>("collab_status"),
  parseLink: (link: string) => invoke<LinkSummary>("collab_parse_link", { link }),
  onStatus: (cb: (s: SessionView) => void) => listen<SessionView>(STATUS_EVENT, cb),
  /** The session is over for this window (the host ended it, or the connection was lost). */
  onEnded: (cb: (e: { reason: string }) => void) => listen<{ reason: string }>(ENDED_EVENT, cb),
  /** Leave the session as a guest; the host's project closes. */
  leave: () => invoke<void>("collab_leave"),
  /** Round trip to the host in milliseconds (guest). */
  ping: () => invoke<number>("collab_ping"),
};
