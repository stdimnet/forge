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

export const STATUS_EVENT = "collab://status";
export const DOC_EVENT = "collab://doc";
export const NOTICE_EVENT = "collab://notice";
export const ENDED_EVENT = "collab://ended";

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
  start: (level: PermissionLevel) => invoke<SessionView>("collab_start", { level }),
  end: () => invoke<void>("collab_end"),
  link: () => invoke<string>("collab_link"),
  join: (link: string) => invoke<SessionView>("collab_join", { link }),
  leave: () => invoke<void>("collab_leave"),
  /** Open a shared document; `content` is the host's editor text of an open tab. */
  docOpen: async (path: string, content?: string) =>
    decodeOpened(await invoke<ArrayBuffer>("collab_doc_open", { path, content: content ?? null })),
  docUpdate: (docId: number, update: Uint8Array) => invoke<void>("collab_doc_update", { docId, update: toBase64(update) }),
  awareness: (docId: number, update: Uint8Array) => invoke<void>("collab_awareness", { docId, update: toBase64(update) }),
  docClose: (docId: number) => invoke<void>("collab_doc_close", { docId }),
  docSave: (docId: number) => invoke<void>("collab_doc_save", { docId }),
  onStatus: (cb: (s: SessionView) => void) => listen<SessionView>(STATUS_EVENT, cb),
  onDoc: (cb: (e: DocEvent) => void) => listen<DocEvent>(DOC_EVENT, cb),
  onNotice: (cb: (e: { text: string }) => void) => listen<{ text: string }>(NOTICE_EVENT, cb),
  onEnded: (cb: (e: { reason: string }) => void) => listen<{ reason: string }>(ENDED_EVENT, cb),
};
