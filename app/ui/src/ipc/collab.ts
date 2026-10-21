/**
 * Code With Me (`app/src-tauri/src/commands/collab.rs`, `crates/forge-collab`). The host
 * shares the files open in its editor; everyone edits them together. Browser mode answers
 * with `mock/collabMock.ts`.
 */
import { invoke, listen } from "./index";

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

/** How a connection travels: straight to the peer, or through a relay server. */
export type PathKind = "direct" | "relay";

/** The path one participant's connection uses (host: each guest, guest: the host). */
export interface ConnectionPath {
  kind: PathKind;
  /** The peer's address (`192.168.1.20:41641`) or the relay's URL. */
  remote: string;
  /** Round-trip time on this path. */
  rttMs?: number;
}

export interface ParticipantConnection extends ConnectionPath {
  participant: number;
}

/** The connection to the session's relay server. */
export interface RelayStatus {
  url: string;
  connected: boolean;
  /** Why the relay cannot be used (token denied, unreachable, certificate). */
  error?: string;
}

export interface RelayTokenStatus {
  set: boolean;
  /** The keychain could not be read or written. */
  error?: string;
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
  /** Direct or relayed, per connected guest (host) or to the host (guest). */
  connections?: ParticipantConnection[];
  /** The relay server of the session, when it uses one. */
  relay?: RelayStatus;
}

/** What the Join Session dialog shows about a pasted link. */
export interface LinkSummary {
  hostName: string;
  hostLabel: string;
  sessionId: string;
  relay?: string;
}

export const STATUS_EVENT = "collab://status";

export const collabApi = {
  status: () => invoke<SessionView>("collab_status"),
  parseLink: (link: string) => invoke<LinkSummary>("collab_parse_link", { link }),
  /** Whether a relay token is stored in the keychain (the token itself never reaches the UI). */
  relayTokenStatus: () => invoke<RelayTokenStatus>("collab_relay_token_status"),
  /** Store the relay token in the keychain; empty or null removes it. */
  setRelayToken: (token: string | null) => invoke<RelayTokenStatus>("collab_set_relay_token", { token }),
  /** Settings | Tools | Code With Me | Test Connection. */
  testRelay: (url: string, publicRelays: boolean) => invoke<RelayStatus>("collab_test_relay", { url, public: publicRelays }),
  onStatus: (cb: (s: SessionView) => void) => listen<SessionView>(STATUS_EVENT, cb),
};
