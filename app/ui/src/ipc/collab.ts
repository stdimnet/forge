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

/** A 1-based caret position (column in UTF-16 units). */
export interface Caret {
  line: number;
  column: number;
}

/** Where a participant is (`forge_collab::presence::Presence`). */
export interface Presence {
  /** Workspace-relative path of the active editor's file; absent when hidden from this window. */
  file?: string;
  caret?: Caret;
  /** The other end of the selection, when text is selected. */
  anchor?: Caret;
  /** First and last line in view. */
  viewport?: { topLine: number; bottomLine: number };
  /** The participant this one follows. */
  following?: number;
}

/** One participant's presence (`collab://presence`, `SessionView.presence`). */
export interface ParticipantPresence extends Presence {
  participant: number;
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
  /** The host's project, open in this window (guest). */
  workspace?: WorkspaceInfo;
  /** Where the other participants are (a guest only learns files it may open). */
  presence?: ParticipantPresence[];
  /** Make Everyone Follow You is on. */
  followEveryone?: boolean;
}

/** One entry of the host's session log. */
export interface LogLine {
  /** Unix time in milliseconds. */
  time: number;
  message: string;
  /** A refused request or a connection without a valid link. */
  warning: boolean;
}

export interface SessionLogView {
  lines: LogLine[];
  /** The log file in Forge's data directory, when the session log is kept. */
  file?: string;
}

/** What the Join Session dialog shows about a pasted link. */
export interface LinkSummary {
  hostName: string;
  hostLabel: string;
  sessionId: string;
  relay?: string;
}

/** The text as last written to disk: `contentHash` of it (`state/editor.ts`). */
export interface SaveState {
  hash: string;
}

/** A shared document as `collab_doc_open` returns it. */
export interface DocOpened {
  docId: number;
  path: string;
  /** The whole document as one Yjs update. */
  update: Uint8Array;
  saved: SaveState;
  readOnly: boolean;
}

/** A change to a shared document (`collab://doc`); updates are base64. */
export interface DocEvent {
  docId: number;
  update?: string;
  awareness?: string;
  saved?: SaveState;
  renamed?: string;
  closed?: string;
}

/** A guest asks to join (`collab://join-request`, host); `code` is what the guest sees too. */
export interface JoinRequest {
  id: number;
  name: string;
  code: string;
  /** The permission level the guest gets when accepted. */
  level: PermissionLevel;
}

/** The guest waits for the host to accept (`collab://waiting`). */
export interface JoiningInfo {
  host: string;
  code: string;
}

export const STATUS_EVENT = "collab://status";
export const JOIN_REQUEST_EVENT = "collab://join-request";
export const JOIN_CLOSED_EVENT = "collab://join-closed";
export const WAITING_EVENT = "collab://waiting";
export const DOC_EVENT = "collab://doc";
export const NOTICE_EVENT = "collab://notice";
export const ENDED_EVENT = "collab://ended";
export const PRESENCE_EVENT = "collab://presence";
export const FOLLOW_HOST_EVENT = "collab://follow-host";

export function toBase64(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin);
}

export function fromBase64(text: string): Uint8Array {
  const bin = atob(text);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** `collab_doc_open`'s raw reply: a big-endian u32 length, a JSON header, the update. */
export function encodeOpened(o: DocOpened): ArrayBuffer {
  const header = new TextEncoder().encode(JSON.stringify({ docId: o.docId, path: o.path, saved: o.saved, readOnly: o.readOnly }));
  const out = new Uint8Array(4 + header.length + o.update.length);
  new DataView(out.buffer).setUint32(0, header.length);
  out.set(header, 4);
  out.set(o.update, 4 + header.length);
  return out.buffer;
}

export function decodeOpened(raw: ArrayBuffer | Uint8Array | number[]): DocOpened {
  const bytes = raw instanceof Uint8Array ? raw : Array.isArray(raw) ? Uint8Array.from(raw) : new Uint8Array(raw);
  const len = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0);
  const header = JSON.parse(new TextDecoder().decode(bytes.subarray(4, 4 + len))) as Omit<DocOpened, "update">;
  return { ...header, update: bytes.slice(4 + len) };
}

export const collabApi = {
  status: () => invoke<SessionView>("collab_status"),
  parseLink: (link: string) => invoke<LinkSummary>("collab_parse_link", { link }),
  /** Start Session: listen for guests and share the files open in the editor. */
  start: (level: PermissionLevel, custom?: Permissions) => invoke<SessionView>("collab_start", { level, custom: custom ?? null }),
  /** Permissions for the whole session: new guests and every guest in it (host). */
  setPermissions: (permissions: Permissions) => invoke<SessionView>("collab_set_permissions", { permissions }),
  /** Permissions of one guest (host). */
  setGuestPermissions: (participant: number, permissions: Permissions) =>
    invoke<void>("collab_set_guest_permissions", { participant, permissions }),
  /** Remove Guest: disconnect a guest at once (host). */
  removeGuest: (participant: number) => invoke<void>("collab_remove_guest", { participant }),
  /** Regenerate Link: the old link stops working; returns the new one (host). */
  regenerateLink: () => invoke<string>("collab_regenerate_link"),
  /** The session log of the current or last hosted session (host). */
  sessionLog: () => invoke<SessionLogView>("collab_session_log"),
  end: () => invoke<void>("collab_end"),
  link: () => invoke<string>("collab_link"),
  join: (link: string) => invoke<SessionView>("collab_join", { link }),
  /** Accept or decline a guest's join request (host). */
  answerJoin: (id: number, accept: boolean) => invoke<void>("collab_answer_join", { id, accept }),
  /** Stop waiting for the host to accept (guest). */
  cancelJoin: () => invoke<void>("collab_cancel_join"),
  leave: () => invoke<void>("collab_leave"),
  /** Open a shared document; `content` is the host's editor text of an open tab. */
  docOpen: async (path: string, content?: string) =>
    decodeOpened(await invoke<ArrayBuffer>("collab_doc_open", { path, content: content ?? null })),
  docUpdate: (docId: number, update: Uint8Array) => invoke<void>("collab_doc_update", { docId, update: toBase64(update) }),
  awareness: (docId: number, update: Uint8Array) => invoke<void>("collab_awareness", { docId, update: toBase64(update) }),
  docClose: (docId: number) => invoke<void>("collab_doc_close", { docId }),
  /** Where this participant is: file, caret, viewport and whom it follows (throttled by the caller). */
  presence: (presence: Presence) => invoke<void>("collab_presence", { presence }),
  /** Make Everyone Follow You on or off (host). */
  followEveryone: (on: boolean) => invoke<SessionView>("collab_follow_everyone", { on }),
  docSave: (docId: number) => invoke<void>("collab_doc_save", { docId }),
  /** Whether a relay token is stored in the keychain (the token itself never reaches the UI). */
  relayTokenStatus: () => invoke<RelayTokenStatus>("collab_relay_token_status"),
  /** Store the relay token in the keychain; empty or null removes it. */
  setRelayToken: (token: string | null) => invoke<RelayTokenStatus>("collab_set_relay_token", { token }),
  /** Settings | Tools | Code With Me | Test Connection. */
  testRelay: (url: string, publicRelays: boolean) => invoke<RelayStatus>("collab_test_relay", { url, public: publicRelays }),
  onStatus: (cb: (s: SessionView) => void) => listen<SessionView>(STATUS_EVENT, cb),
  onDoc: (cb: (e: DocEvent) => void) => listen<DocEvent>(DOC_EVENT, cb),
  onJoinRequest: (cb: (r: JoinRequest) => void) => listen<JoinRequest>(JOIN_REQUEST_EVENT, cb),
  /** A request is gone without an answer (it timed out, or the session ended). */
  onJoinClosed: (cb: (e: { id: number }) => void) => listen<{ id: number }>(JOIN_CLOSED_EVENT, cb),
  onWaiting: (cb: (w: JoiningInfo) => void) => listen<JoiningInfo>(WAITING_EVENT, cb),
  onNotice: (cb: (e: { text: string }) => void) => listen<{ text: string }>(NOTICE_EVENT, cb),
  onEnded: (cb: (e: { reason: string }) => void) => listen<{ reason: string }>(ENDED_EVENT, cb),
  /** Another participant moved. */
  onPresence: (cb: (p: ParticipantPresence) => void) => listen<ParticipantPresence>(PRESENCE_EVENT, cb),
  /** The host turned Make Everyone Follow You on or off (guest). */
  onFollowHost: (cb: (e: { on: boolean }) => void) => listen<{ on: boolean }>(FOLLOW_HOST_EVENT, cb),
  /** Round trip to the host in milliseconds (guest). */
  ping: () => invoke<number>("collab_ping"),
};
