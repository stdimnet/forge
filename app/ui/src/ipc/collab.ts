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
  /** The guest's connection dropped; the host keeps its place while it reconnects. */
  reconnecting?: boolean;
}

/** A guest's connection to the host (`forge_collab::reconnect::ConnectionState`). */
export type ConnectionState =
  | { state: "connected" }
  | {
      state: "reconnecting";
      /** 1-based attempt that starts in `nextRetrySecs`. */
      attempt: number;
      nextRetrySecs: number;
      /** Why the previous attempt failed. */
      error?: string;
    };

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
  /** Shared terminals: the ones the host shares (with `key`), or the host's terminals this guest sees. */
  terminals?: SharedTerminal[];
  /** The host's runs this guest sees (guest). */
  runs?: SharedRun[];
  /** Whether the connection to the host is up or being restored (guest). */
  connection?: ConnectionState;
}

/** A shared terminal (`forge_collab::share::SharedTerminal`). */
export interface SharedTerminal {
  id: number;
  /** The host's tab title, or "<guest>: Local" for a guest's terminal on the host. */
  title: string;
  /** Who opened it: the host (0) or a guest. */
  owner: number;
  /** The host's size; guests render at it. */
  cols: number;
  rows: number;
  running: boolean;
  exitCode?: number;
  /** The terminal's id in this window (host). */
  key?: string;
}

/** One of the host's runs as a guest sees it (`forge_collab::share::SharedRun`). */
export interface SharedRun {
  id: number;
  /** The run configuration's name. */
  title: string;
  /** The configuration's type id. */
  kind?: string;
  /** Who started it (the host, or the guest who asked). */
  startedBy: number;
  running: boolean;
  exitCode?: number;
}

/** What a guest's tab of a shared terminal starts with. */
export interface TerminalAttach {
  scrollback: string;
  cols: number;
  rows: number;
  running: boolean;
}

/** A guest asks the host to run a configuration (`collab://run-request`, host). */
export interface RunRequest {
  request: number;
  configuration: string;
  guest: string;
}

/** The run id a guest's Run tool window uses for one of the host's runs. */
export const sharedRunId = (run: number) => `cwm-run-${run}`;

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
  /** The host's machine as the link names it. */
  machine?: string;
  /** `name` or `name (machine)`. */
  hostLabel: string;
  sessionId: string;
  relay?: string;
  /** The name this window joins with (Settings | Tools | Code With Me | User name). */
  userName: string;
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

/** `collab_doc_sync`'s reply: what the replica missed after a reconnect. */
export interface DocSynced extends DocOpened {
  /** The host's state vector: send it what it is missing. */
  stateVector: Uint8Array;
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
export const RUN_REQUEST_EVENT = "collab://run-request";
export const TERMINAL_OPENED_EVENT = "collab://terminal-opened";
export const RESUMED_EVENT = "collab://resumed";

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

/** `collab_doc_sync`'s raw reply: like `collab_doc_open`'s, with the host's state vector. */
export function encodeSynced(s: DocSynced): ArrayBuffer {
  const { stateVector, ...opened } = s;
  const header = new TextEncoder().encode(
    JSON.stringify({ docId: opened.docId, path: opened.path, saved: opened.saved, readOnly: opened.readOnly, stateVector: toBase64(stateVector) }),
  );
  const out = new Uint8Array(4 + header.length + opened.update.length);
  new DataView(out.buffer).setUint32(0, header.length);
  out.set(header, 4);
  out.set(opened.update, 4 + header.length);
  return out.buffer;
}

export function decodeSynced(raw: ArrayBuffer | Uint8Array | number[]): DocSynced {
  const opened = decodeOpened(raw) as DocOpened & { stateVector: unknown };
  return { ...opened, stateVector: fromBase64(String(opened.stateVector ?? "")) };
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
  /** Bring a document up to date after the connection was restored (guest). */
  docSync: async (docId: number, stateVector: Uint8Array) =>
    decodeSynced(await invoke<ArrayBuffer>("collab_doc_sync", { docId, stateVector: toBase64(stateVector) })),
  /** The connection to the host was restored after it dropped (guest). */
  onResumed: (cb: () => void) => listen<unknown>(RESUMED_EVENT, () => cb()),
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
  /** Share With Guests on a terminal tab, or stop sharing it (host). */
  shareTerminal: (id: string, title: string, shared: boolean) => invoke<SessionView>("collab_share_terminal", { id, title, shared }),
  /** Show one of the host's terminals in the tab `id`; it then works like any terminal session (guest). */
  terminalAttach: (terminal: number, id: string) => invoke<TerminalAttach>("collab_terminal_attach", { terminal, id }),
  /** Show one of the host's runs: its events so far; live events follow on `run://cwm-run-<run>` (guest). */
  runAttach: (run: number) => invoke<import("./run").RunEvent[]>("collab_run_attach", { run }),
  runDetach: (run: number) => invoke<void>("collab_run_detach", { run }),
  /** Run one of the host's configurations by name (guest, Run Full access). */
  runStart: (configuration: string) => invoke<void>("collab_run_start", { configuration }),
  /** Stop one of the host's runs (guest, Run Full access). */
  runStop: (run: number) => invoke<void>("collab_run_stop", { run }),
  /** Console input for one of the host's runs (guest, Run Full access). */
  runInput: (run: number, data: string) => invoke<void>("collab_run_input", { run, data }),
  /** The host's UI started (or could not start) a run a guest asked for (host). */
  runAnswer: (request: number, error?: string) => invoke<void>("collab_run_answer", { request, error: error ?? null }),
  /** A guest asks to run a configuration (host). */
  onRunRequest: (cb: (r: RunRequest) => void) => listen<RunRequest>(RUN_REQUEST_EVENT, cb),
  /** A guest opened a terminal on this machine (host). */
  onTerminalOpened: (cb: (e: { id: string; title: string }) => void) => listen<{ id: string; title: string }>(TERMINAL_OPENED_EVENT, cb),
};
