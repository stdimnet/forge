/**
 * Browser-mode Code With Me. A small host runs in the page with real Yjs documents, so the
 * editor binding, carets and undo work as in the desktop app:
 *
 * - Start Session in a plain browser page: "Alex" (a scripted guest inside the page) joins,
 *   opens `OrderController.php`, moves the caret, types and selects a block.
 * - `?collab=host` and `?collab=guest` in two pages: the pages exchange Yjs updates. Pages
 *   of one browser talk through a `BroadcastChannel`; Playwright scenarios with two browser
 *   contexts bridge them through `window.__cwmOut` / `window.__cwmReceive`.
 *
 * Nothing leaves the browser, and saving writes the in-memory demo files only.
 */
import * as Y from "yjs";
import { Awareness, applyAwarenessUpdate, encodeAwarenessUpdate } from "y-protocols/awareness";
import { emitMock, invoke, registerMock } from "@/ipc";
import {
  encodeOpened,
  fromBase64,
  levelOf,
  PRESETS,
  STATUS_EVENT,
  toBase64,
  type DocOpened,
  type Participant,
  type PermissionLevel,
  type Permissions,
  type Presence,
  type RelayStatus,
  type RelayTokenStatus,
  type SessionView,
} from "@/ipc/collab";
import { contentHash } from "@/state/editor";

const NOTICE = "Browser mode: the session runs inside this browser; nothing goes over the network.";
const HOST_NAME = "Robert";
const demoLink = (secret: string) => `forge://cwm/join?v=1&name=Robert&m=demo&s=browsermodedemo0000000000#h=demo&k=${secret}&a=127.0.0.1:41641`;
let link = demoLink("demo");
const mode = typeof location !== "undefined" ? new URLSearchParams(location.search).get("collab") : null;

// ---- the wire between pages ---------------------------------------------------------------

/** Messages between a mock host and mock guests; updates are base64. */
interface WireMsg {
  from: string;
  to: string;
  kind:
    | "knock"
    | "welcome"
    | "open"
    | "opened"
    | "error"
    | "update"
    | "awareness"
    | "save"
    | "saved"
    | "close"
    | "leave"
    | "status"
    | "ended"
    | "waiting"
    | "refused"
    | "presence"
    | "follow-host";
  [key: string]: unknown;
}

const pageId = `page-${Math.random().toString(36).slice(2, 10)}`;
type Bridge = { __cwmOut?: (m: WireMsg) => void; __cwmReceive?: (m: WireMsg) => void };
const bridge = (typeof window !== "undefined" ? window : {}) as Bridge;
const channel = !bridge.__cwmOut && typeof BroadcastChannel !== "undefined" ? new BroadcastChannel("forge-cwm-demo") : null;

function send(m: Omit<WireMsg, "from">): void {
  const msg = { ...m, from: pageId } as WireMsg;
  if (bridge.__cwmOut) bridge.__cwmOut(msg);
  else channel?.postMessage(msg);
}

function receive(m: WireMsg): void {
  if (m.to !== pageId && m.to !== "*") return;
  if (role === "host") void hostReceive(m.from, m);
  else if (role === "guest" || joining) guestReceive(m);
}
bridge.__cwmReceive = receive;
channel?.addEventListener("message", (e) => receive(e.data as WireMsg));

// ---- shared state -------------------------------------------------------------------------

let role: "none" | "host" | "guest" = "none";
let joining = false;
let permissions: Permissions = PRESETS.readOnly;
let participants: Participant[] = [];
let files: { path: string }[] = [];
let me = 0;
/** Where everyone is, by participant id (the host's view: every file). */
const presences = new Map<number, Presence>();
let followEveryone = false;

/** The host's session log (in memory; the app writes it to its data directory). */
const logLines: { time: number; message: string; warning: boolean }[] = [];
const log = (message: string, warning = false) => logLines.push({ time: Date.now(), message, warning });
const levelTitle = (p: Permissions) => ({ readOnly: "Read-only", editFiles: "Edit files", fullAccess: "Full access", custom: "Custom" })[levelOf(p)];

function view(): SessionView {
  if (role === "none") return { role, canHost: true, participants: [], files: [] };
  return {
    role,
    canHost: role === "host",
    link: role === "host" ? link : undefined,
    me,
    participants,
    files,
    permissions: role === "host" ? permissions : participants.find((p) => p.id === me)?.permissions,
    hostLabel: role === "guest" ? `${HOST_NAME} (demo)` : undefined,
    notice: NOTICE,
    presence: [...presences].filter(([id]) => id !== me).map(([participant, p]) => ({ participant, ...p })),
    followEveryone,
  };
}

/** Secret files the mock host never names to guests (like `hostOpen` refuses them). */
const hiddenFile = (path: string) => /(^|\/)\.env($|\.)/.test(path) && !/\.env\.(example|dist)$/.test(path);

/** A presence as a guest may see it. */
const visible = (p: Presence): Presence => (p.file && hiddenFile(p.file) ? (p.following === undefined ? {} : { following: p.following }) : p);

/** Store a participant's presence and pass it on (host). */
function hostPresence(participant: number, p: Presence): void {
  presences.set(participant, p);
  for (const g of guests.values()) {
    if (g.participant.id !== participant) toGuest(g.key, { kind: "presence", participant, presence: visible(p) });
  }
  if (participant !== 0) emitMock("collab://presence", { participant, ...p });
}

const emitStatus = () => emitMock("collab://status", view());
const emitDoc = (e: Record<string, unknown>) => emitMock("collab://doc", e);

// ---- host -----------------------------------------------------------------------------------

interface HostDoc {
  id: number;
  path: string;
  ydoc: Y.Doc;
  saved: string;
  /** "host", guest page ids, or "alex" for the scripted guest. */
  openBy: Set<string>;
}

interface MockGuest {
  key: string;
  participant: Participant;
}

const docs = new Map<number, HostDoc>();
const guests = new Map<string, MockGuest>();
let nextDoc = 1;
let nextParticipant = 1;

const hostParticipant = (): Participant => ({ id: 0, name: HOST_NAME, role: "host", color: 1, permissions: PRESETS.fullAccess });
const docByPath = (path: string) => [...docs.values()].find((d) => d.path === path);
const opened = (d: HostDoc, readOnly = false): DocOpened => ({
  docId: d.id,
  path: d.path,
  update: Y.encodeStateAsUpdate(d.ydoc),
  saved: { hash: contentHash(d.saved) },
  readOnly,
});

function hostChanged(): void {
  participants = [hostParticipant(), ...[...guests.values()].map((g) => g.participant)];
  files = [...docs.values()].filter((d) => d.openBy.has("host")).map((d) => ({ path: d.path })).sort((a, b) => a.path.localeCompare(b.path));
  for (const g of guests.values()) toGuest(g.key, { kind: "status", participants, files });
  emitStatus();
}

/** Send to a guest: over the wire, or to the scripted guest in this page. */
function toGuest(key: string, m: Omit<WireMsg, "from" | "to">): void {
  if (key === "alex") alex?.receive(m as WireMsg);
  else send({ ...m, to: key });
}

/** Forward to everyone with the document open except `except` (the host's editor included). */
function fanOut(d: HostDoc, except: string, m: Omit<WireMsg, "from" | "to">, toHostUi: Record<string, unknown> | null): void {
  for (const key of d.openBy) if (key !== except && key !== "host") toGuest(key, m);
  if (toHostUi && except !== "host" && d.openBy.has("host")) emitDoc({ docId: d.id, ...toHostUi });
}

async function hostOpen(path: string, content?: string): Promise<DocOpened> {
  if (/(^|\/)\.env($|\.)/.test(path) && !/\.env\.(example|dist)$/.test(path)) throw new Error(`"${path}" is not shared with guests`);
  let d = docByPath(path);
  if (!d) {
    const disk = String(await invoke<string>("fs_read_file", { path }));
    d = { id: nextDoc++, path, ydoc: new Y.Doc(), saved: disk, openBy: new Set() };
    d.ydoc.getText("content").insert(0, content ?? disk);
    docs.set(d.id, d);
  }
  d.openBy.add("host");
  hostChanged();
  return opened(d);
}

function hostApply(from: string, docId: number, update: Uint8Array): void {
  const d = docs.get(docId);
  if (!d || !d.openBy.has(from)) return;
  if (from !== "host" && guests.get(from)?.participant.permissions.files !== "edit") {
    toGuest(from, { kind: "error", text: "Your permission is Read-only" });
    return;
  }
  Y.applyUpdate(d.ydoc, update, from);
  fanOut(d, from, { kind: "update", docId, update: toBase64(update) }, { update: toBase64(update) });
}

function hostAwareness(from: string, docId: number, update: Uint8Array): void {
  const d = docs.get(docId);
  if (!d || !d.openBy.has(from)) return;
  fanOut(d, from, { kind: "awareness", docId, update: toBase64(update) }, { awareness: toBase64(update) });
}

async function hostSave(docId: number): Promise<void> {
  const d = docs.get(docId);
  if (!d) throw new Error("the file is no longer shared");
  const text = d.ydoc.getText("content").toString();
  await invoke("fs_write_file", { path: d.path, contents: text });
  d.saved = text;
  const saved = { hash: contentHash(text) };
  for (const key of d.openBy) if (key !== "host") toGuest(key, { kind: "saved", docId, saved });
  if (d.openBy.has("host")) emitDoc({ docId, saved });
}

function hostClose(who: string, docId: number): void {
  const d = docs.get(docId);
  if (!d) return;
  d.openBy.delete(who);
  if (d.openBy.size === 0 && d.ydoc.getText("content").toString() === d.saved) docs.delete(docId);
  if (who === "host") hostChanged();
}

/** Join requests the host has not answered yet, by id. */
const requests = new Map<number, (accept: boolean) => void>();
let nextRequest = 1;
const MOCK_CODE = "482 913";

/** The host's approval: shows the request and waits for `collab_answer_join` (2 minutes). */
function askHost(name: string): Promise<boolean> {
  const id = nextRequest++;
  return new Promise((resolve) => {
    const finish = (accept: boolean) => {
      requests.delete(id);
      clearTimeout(timer);
      resolve(accept);
    };
    const timer = setTimeout(() => {
      emitMock("collab://join-closed", { id });
      finish(false);
    }, 120_000);
    requests.set(id, finish);
    emitMock("collab://join-request", { id, name, code: MOCK_CODE, level: levelOf(permissions) });
  });
}

function admit(key: string, name: string): MockGuest {
  const participant: Participant = { id: nextParticipant, name, role: "guest", color: 2 + ((nextParticipant - 1) % 7), permissions };
  nextParticipant++;
  log(`${name} joined (${levelTitle(permissions)})`);
  const g = { key, participant };
  guests.set(key, g);
  hostChanged();
  emitMock("collab://notice", { text: `${name} joined the session.` });
  return g;
}

function dropGuest(key: string, removed = false): void {
  const g = guests.get(key);
  if (!g) return;
  if (!removed) log(`${g.participant.name} left`);
  guests.delete(key);
  presences.delete(g.participant.id);
  for (const d of [...docs.values()]) if (d.openBy.has(key)) hostClose(key, d.id);
  hostChanged();
  emitMock("collab://notice", { text: `${g.participant.name} left the session.` });
}

async function hostReceive(from: string, m: WireMsg): Promise<void> {
  if (m.kind === "knock") {
    let g = guests.get(from);
    if (!g) {
      toGuest(from, { kind: "waiting" });
      if (!(await askHost(String(m.name ?? "Guest")))) return void toGuest(from, { kind: "refused", reason: "The host declined your request." });
      g = admit(from, String(m.name ?? "Guest"));
    }
    const snapshot = [...presences].filter(([id]) => id !== g!.participant.id).map(([participant, p]) => ({ participant, ...visible(p) }));
    toGuest(from, { kind: "welcome", you: g.participant.id, participants, files, presence: snapshot, followEveryone });
    return;
  }
  if (!guests.has(from)) return;
  const docId = Number(m.docId);
  switch (m.kind) {
    case "open": {
      const d = docByPath(String(m.path));
      if (!d || !d.openBy.has("host")) toGuest(from, { kind: "opened", req: m.req, error: "The host does not share this file." });
      else {
        d.openBy.add(from);
        const o = opened(d, guests.get(from)!.participant.permissions.files !== "edit");
        toGuest(from, { kind: "opened", req: m.req, docId: o.docId, path: o.path, saved: o.saved, readOnly: o.readOnly, update: toBase64(o.update) });
      }
      break;
    }
    case "update":
      hostApply(from, docId, fromBase64(String(m.update)));
      break;
    case "awareness":
      hostAwareness(from, docId, fromBase64(String(m.update)));
      break;
    case "save":
      if (guests.get(from)!.participant.permissions.files !== "edit") {
        log(`Refused save of ${guests.get(from)!.participant.name}: Your permission is Read-only`, true);
        toGuest(from, { kind: "saved", req: m.req, error: "Your permission is Read-only" });
      }
      else await hostSave(docId).then(() => toGuest(from, { kind: "saved", req: m.req }));
      break;
    case "close":
      hostClose(from, docId);
      break;
    case "leave":
      dropGuest(from);
      break;
    case "presence":
      hostPresence(guests.get(from)!.participant.id, m.presence as Presence);
      break;
  }
}

function startHost(level: PermissionLevel, custom?: Permissions): SessionView {
  role = "host";
  me = 0;
  link = demoLink("demo");
  permissions = level === "custom" ? (custom ?? PRESETS.readOnly) : PRESETS[level];
  logLines.length = 0;
  log(`${HOST_NAME} started the session (guests: ${levelTitle(permissions)})`);
  hostChanged();
  return view();
}

function endHost(reason: string): void {
  log(`Session ended: ${reason}`);
  for (const answer of [...requests.values()]) answer(false);
  for (const key of guests.keys()) toGuest(key, { kind: "ended", reason });
  guests.clear();
  docs.clear();
  presences.clear();
  followEveryone = false;
  alex?.stop();
  alex = null;
  role = "none";
  participants = [];
  files = [];
  emitStatus();
}

// ---- the scripted guest "Alex" --------------------------------------------------------------

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** A guest inside this page with its own Yjs replica, for the single-page demo. */
class ScriptedGuest {
  private doc: Y.Doc | null = null;
  private awareness: Awareness | null = null;
  private docId = 0;
  private stopped = false;

  receive(m: WireMsg): void {
    if (m.kind === "update" && this.doc && Number(m.docId) === this.docId) Y.applyUpdate(this.doc, fromBase64(String(m.update)), "remote");
    if (m.kind === "awareness" && this.awareness && Number(m.docId) === this.docId) applyAwarenessUpdate(this.awareness, fromBase64(String(m.update)), "remote");
  }

  stop(): void {
    this.stopped = true;
    this.awareness?.destroy();
  }

  async run(): Promise<void> {
    if (!(await askHost("Alex")) || this.stopped) return;
    const g = admit("alex", "Alex");
    await sleep(400);
    const target = docByPath("app/Http/Controllers/OrderController.php") ?? [...docs.values()].find((d) => d.openBy.has("host"));
    if (!target || this.stopped) return;
    target.openBy.add("alex");
    this.docId = target.id;
    this.doc = new Y.Doc();
    Y.applyUpdate(this.doc, Y.encodeStateAsUpdate(target.ydoc), "remote");
    const text = this.doc.getText("content");
    this.doc.on("update", (u: Uint8Array, origin: unknown) => origin !== "remote" && hostApply("alex", this.docId, u));
    this.awareness = new Awareness(this.doc);
    this.awareness.setLocalStateField("user", { name: g.participant.name, color: `var(--collab-${g.participant.color})`, colorLight: `color-mix(in srgb, var(--collab-${g.participant.color}) 25%, transparent)`, colorIndex: g.participant.color });
    this.awareness.on("update", ({ added, updated }: { added: number[]; updated: number[] }, origin: unknown) => {
      if (origin === "remote" || !this.awareness) return;
      const changed = [...added, ...updated].filter((c) => c === this.doc!.clientID);
      if (changed.length) hostAwareness("alex", this.docId, encodeAwarenessUpdate(this.awareness, changed));
    });
    const caret = (anchor: number, head = anchor) =>
      this.awareness?.setLocalStateField("cursor", {
        anchor: Y.relativePositionToJSON(Y.createRelativePositionFromTypeIndex(text, anchor)),
        head: Y.relativePositionToJSON(Y.createRelativePositionFromTypeIndex(text, head)),
      });
    const find = (needle: string) => text.toString().indexOf(needle);
    // Alex's presence: the file, the caret's line and column, and the lines in view.
    const where = (offset: number) => {
      const before = text.toString().slice(0, offset).split("\n");
      const line = before.length;
      hostPresence(g.participant.id, {
        file: target.path,
        caret: { line, column: before[before.length - 1].length + 1 },
        viewport: { topLine: Math.max(1, line - 8), bottomLine: line + 22 },
      });
    };
    // Alex goes to the discount line and adds a comment, then selects the payable block.
    const at = find("0.1 : 0.0;");
    if (at < 0) return;
    let pos = at + "0.1 : 0.0;".length;
    caret(pos);
    where(pos);
    await sleep(500);
    for (const ch of " // VIP customers") {
      if (this.stopped) return;
      text.insert(pos, ch);
      pos += ch.length;
      caret(pos);
      await sleep(45);
    }
    await sleep(400);
    const from = find("        if ($order->isPayable())");
    const to = find("        return response()");
    if (from >= 0 && to > from) {
      caret(from, to - 1);
      where(from);
    }
  }
}

let alex: ScriptedGuest | null = null;

// ---- guest (another page hosts) -------------------------------------------------------------

let hostKey = "*";
const pending = new Map<number, (m: WireMsg) => void>();
let nextReq = 1;

function request(m: Omit<WireMsg, "from" | "to" | "req">): Promise<WireMsg> {
  const req = nextReq++;
  return new Promise((resolve, reject) => {
    pending.set(req, resolve);
    send({ ...m, to: hostKey, req } as Omit<WireMsg, "from">);
    setTimeout(() => pending.delete(req) && reject(new Error("The host did not answer.")), 5000);
  });
}

function guestReceive(m: WireMsg): void {
  if (m.req !== undefined && pending.has(Number(m.req))) {
    pending.get(Number(m.req))!(m);
    pending.delete(Number(m.req));
    return;
  }
  switch (m.kind) {
    case "welcome":
      hostKey = m.from;
      joinResolve?.(m);
      break;
    case "waiting":
      waitingForUser?.();
      break;
    case "refused":
      joinReject?.(new Error(String(m.reason)));
      break;
    case "status":
      participants = m.participants as Participant[];
      files = m.files as { path: string }[];
      emitStatus();
      break;
    case "update":
      emitDoc({ docId: m.docId, update: m.update });
      break;
    case "awareness":
      emitDoc({ docId: m.docId, awareness: m.update });
      break;
    case "saved":
      emitDoc({ docId: m.docId, saved: m.saved });
      break;
    case "presence":
      presences.set(Number(m.participant), m.presence as Presence);
      emitMock("collab://presence", { participant: Number(m.participant), ...(m.presence as Presence) });
      break;
    case "follow-host":
      followEveryone = !!m.on;
      emitMock("collab://follow-host", { on: followEveryone });
      emitStatus();
      break;
    case "error":
      emitMock("collab://notice", { text: String(m.text) });
      break;
    case "ended":
      role = "none";
      participants = [];
      files = [];
      presences.clear();
      followEveryone = false;
      emitMock("collab://ended", { reason: String(m.reason) });
      emitStatus();
      break;
  }
}

let joinResolve: ((m: WireMsg) => void) | null = null;
let joinReject: ((e: Error) => void) | null = null;
let waitingForUser: (() => void) | null = null;

async function join(name: string): Promise<SessionView> {
  joining = true;
  try {
    // The host answers the knock only after its user accepted, so the wait is the approval's.
    const welcome = await new Promise<WireMsg>((resolve, reject) => {
      joinResolve = resolve;
      joinReject = reject;
      send({ kind: "knock", to: "*", name });
      const quiet = setTimeout(() => reject(new Error("No host answered. Start a session in another page of this browser first (?collab=host).")), 3000);
      // A host page that is asking its user answers with `waiting`.
      waitingForUser = () => {
        clearTimeout(quiet);
        emitMock("collab://waiting", { host: HOST_NAME, code: MOCK_CODE });
      };
    });
    role = "guest";
    me = Number(welcome.you);
    participants = welcome.participants as Participant[];
    files = welcome.files as { path: string }[];
    presences.clear();
    for (const { participant, ...p } of (welcome.presence as ({ participant: number } & Presence)[]) ?? []) presences.set(participant, p);
    followEveryone = !!welcome.followEveryone;
    emitStatus();
    return view();
  } finally {
    joining = false;
    joinResolve = null;
    joinReject = null;
    waitingForUser = null;
  }
}

// ---- commands -------------------------------------------------------------------------------

registerMock("collab_status", () => view());

registerMock("collab_parse_link", ({ link }) => {
  const text = String(link ?? "").trim();
  if (!text.startsWith("forge://cwm/join?") && !text.startsWith("cwm1:")) throw new Error("Invalid session link: not a Code With Me link");
  const name = decodeURIComponent(/[?&:]name=([^&#]*)/.exec(text)?.[1] ?? "");
  if (!name) throw new Error("Invalid session link: name is missing");
  return { hostName: name, hostLabel: name, sessionId: /[?&]s=([^&#]*)/.exec(text)?.[1] ?? "" };
});

registerMock("collab_start", ({ level, custom }) => {
  if (role !== "none") throw new Error("A Code With Me session is already running.");
  const v = startHost((level as PermissionLevel) ?? "readOnly", (custom as Permissions | null) ?? undefined);
  if (mode !== "host") {
    alex = new ScriptedGuest();
    setTimeout(() => void alex?.run(), 1000);
  }
  return v;
});

registerMock("collab_end", () => endHost("The host ended the session."));
registerMock("collab_answer_join", ({ id, accept }) => {
  const answer = requests.get(Number(id));
  if (!answer) throw new Error("The request is no longer open.");
  answer(Boolean(accept));
});
registerMock("collab_cancel_join", () => {
  joinReject?.(new Error("Cancelled."));
});
registerMock("collab_link", () => link);
registerMock("collab_session_log", () => ({ lines: [...logLines] }));
registerMock("collab_regenerate_link", () => {
  if (role !== "host") throw new Error("No session is running.");
  link = demoLink(Math.random().toString(36).slice(2, 10));
  log("Regenerated the session link");
  emitStatus();
  return link;
});

/** New permissions for some guests; they are told through the participant list. */
function setGuestPermissions(keys: string[], p: Permissions): void {
  for (const key of keys) {
    const g = guests.get(key);
    if (g) g.participant = { ...g.participant, permissions: p };
  }
  hostChanged();
}

const guestKey = (participant: unknown) => [...guests.values()].find((g) => g.participant.id === Number(participant))?.key;

registerMock("collab_set_permissions", ({ permissions: p }) => {
  if (role !== "host") throw new Error("No session is running.");
  permissions = p as Permissions;
  log(`Permission of all guests: ${levelTitle(permissions)}`);
  setGuestPermissions([...guests.keys()], permissions);
  return view();
});
registerMock("collab_set_guest_permissions", ({ participant, permissions: p }) => {
  const key = guestKey(participant);
  if (role !== "host" || !key) throw new Error("The guest is no longer in the session.");
  log(`Permission of ${guests.get(key)!.participant.name}: ${levelTitle(p as Permissions)}`);
  setGuestPermissions([key], p as Permissions);
});
registerMock("collab_remove_guest", ({ participant }) => {
  const key = guestKey(participant);
  if (role !== "host" || !key) throw new Error("The guest is no longer in the session.");
  log(`Removed ${guests.get(key)!.participant.name}`);
  toGuest(key, { kind: "ended", reason: "The host removed you from the session." });
  if (key === "alex") {
    alex?.stop();
    alex = null;
  }
  dropGuest(key, true);
});

registerMock("collab_join", async ({ link }) => {
  if (role !== "none") throw new Error("A Code With Me session is already running.");
  await invoke("collab_parse_link", { link });
  return join("Alex");
});

registerMock("collab_leave", () => {
  if (role !== "guest") return;
  send({ kind: "leave", to: hostKey });
  role = "none";
  participants = [];
  files = [];
  presences.clear();
  followEveryone = false;
  emitStatus();
});

registerMock("collab_doc_open", async ({ path, content }) => {
  if (role === "host") return encodeOpened(await hostOpen(String(path), content == null ? undefined : String(content)));
  if (role !== "guest") throw new Error("No session is running.");
  const r = await request({ kind: "open", path });
  if (r.error) throw new Error(String(r.error));
  return encodeOpened({ docId: Number(r.docId), path: String(r.path), saved: r.saved as { hash: string }, readOnly: !!r.readOnly, update: fromBase64(String(r.update)) });
});

registerMock("collab_doc_update", ({ docId, update }) => {
  if (role === "host") hostApply("host", Number(docId), fromBase64(String(update)));
  else if (role === "guest") send({ kind: "update", to: hostKey, docId, update });
});

registerMock("collab_awareness", ({ docId, update }) => {
  if (role === "host") hostAwareness("host", Number(docId), fromBase64(String(update)));
  else if (role === "guest") send({ kind: "awareness", to: hostKey, docId, update });
});

registerMock("collab_presence", ({ presence }) => {
  if (role === "host") hostPresence(0, presence as Presence);
  else if (role === "guest") send({ kind: "presence", to: hostKey, presence });
});

registerMock("collab_follow_everyone", ({ on }) => {
  if (role !== "host") throw new Error("No session is running.");
  followEveryone = !!on;
  for (const g of guests.values()) toGuest(g.key, { kind: "follow-host", on: followEveryone });
  log(followEveryone ? "Asked everyone to follow" : "Stopped asking everyone to follow");
  emitStatus();
  return view();
});

registerMock("collab_doc_close", ({ docId }) => {
  if (role === "host") hostClose("host", Number(docId));
  else if (role === "guest") send({ kind: "close", to: hostKey, docId });
});

registerMock("collab_doc_save", async ({ docId }) => {
  if (role === "host") return hostSave(Number(docId));
  const r = await request({ kind: "save", docId });
  if (r.error) throw new Error(String(r.error));
});

// `?collab=host` starts hosting with Edit files; `?collab=guest` joins the host page.
if (mode === "host") startHost("editFiles");
if (mode === "guest") {
  void (async () => {
    // Join once the workbench is up, so opening the project does not reset the shared tabs.
    for (let i = 0; i < 40 && !document.querySelector(".cm-content"); i++) await sleep(250);
    await sleep(500);
    for (let i = 0; i < 20 && role === "none"; i++) {
      try {
        await join("Alex");
      } catch {
        await sleep(500);
      }
    }
  })();
}

// ---- relay (Settings | Tools | Code With Me | Relay Server) ------------------------------

let token = false;
const tokenStatus = (): RelayTokenStatus => ({ set: token });

registerMock("collab_relay_token_status", () => tokenStatus());

registerMock("collab_set_relay_token", ({ token: raw }) => {
  const value = String(raw ?? "").trim();
  if (value && !/^[\x21-\x7e]+$/.test(value)) throw new Error("Invalid relay server: the relay token must be printable ASCII without spaces");
  token = value !== "";
  return tokenStatus();
});

registerMock("collab_test_relay", ({ url, public: publicRelays }) => {
  const text = String(url ?? "").trim();
  if (!text && !publicRelays) throw new Error("Enter a relay server first.");
  if (!text) return { url: "https://relay.iroh.example/", connected: true } satisfies RelayStatus;
  const full = text.includes("://") ? text : `https://${text}`;
  if (!/^https?:\/\/[^\s/?#@]+(\/.*)?$/.test(full)) throw new Error("Invalid relay server: the URL has no host name");
  if (full.includes("unreachable")) throw new Error("Cannot reach the relay server (the relay did not answer)");
  return { url: full.replace(/\/+$/, ""), connected: true } satisfies RelayStatus;
});

// Screenshot and test hook: show a session in the popup and the status bar.
declare global {
  interface Window {
    __forgeCollabMock?: { show: (view: SessionView) => void };
  }
}
window.__forgeCollabMock = { show: (view) => emitMock(STATUS_EVENT, view) };
