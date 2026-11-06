/**
 * Shared documents in this window: one Yjs document per shared editor tab, with its
 * awareness (carets, selections) and an undo manager that only tracks this participant's
 * edits. Remote updates are applied with the `REMOTE` origin, which no undo manager tracks,
 * so Ctrl+Z never undoes someone else's typing.
 *
 * Tab paths: the host's shared tabs keep their project paths; a guest's tabs are
 * `cwm://<path>` so they never mix with the guest's own project.
 */
import * as Y from "yjs";
import { Awareness, applyAwarenessUpdate, encodeAwarenessUpdate, modifyAwarenessUpdate, removeAwarenessStates } from "y-protocols/awareness";
import { collabApi, fromBase64, type DocEvent, type DocOpened, type DocSynced, type Participant, type ViewOnly } from "@/ipc/collab";
import { withAuthor, type Author } from "./guestChanges";
import { session } from "./store";
import { t } from "@/i18n";

import { GUEST_PREFIX } from "@/ipc/projectPath";

export { GUEST_PREFIX };

/** Origin of updates that came from the session (other participants, the disk). */
export const REMOTE = Symbol("collab-remote");

export interface SharedDoc {
  docId: number;
  /** The editor tab showing it. */
  tabPath: string;
  ydoc: Y.Doc;
  ytext: Y.Text;
  awareness: Awareness;
  undo: Y.UndoManager;
  savedHash: string;
  readOnly: boolean;
  /** Read-only for everyone: too large to co-edit, or binary (`ViewOnly`). */
  view?: ViewOnly;
  dispose: () => void;
}

const byId = new Map<number, SharedDoc>();
const byTab = new Map<string, SharedDoc>();
/** Events that arrived before the document was opened here. */
const early = new Map<number, DocEvent[]>();

type Listener = (doc: SharedDoc, e: DocEvent) => void;
const listeners = new Set<Listener>();

let conflictListener: ((e: DocEvent) => void) | null = null;

/** File Cache Conflicts (`conflict.ts`); they also concern documents not open here. */
export function setConflictListener(cb: (e: DocEvent) => void): void {
  conflictListener = cb;
}

/** Saves, renames and closes of shared documents (for the editor tabs). */
export function onSharedDocEvent(cb: Listener): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

export const sharedDoc = (tabPath: string): SharedDoc | undefined => byTab.get(tabPath);
export const sharedDocById = (docId: number): SharedDoc | undefined => byId.get(docId);
export const sharedDocs = (): SharedDoc[] => [...byId.values()];

/** Colors come from the theme; nothing a peer sends ends up in a style attribute. */
export function userStyle(colorIndex: number): { color: string; colorLight: string } {
  const n = Math.min(8, Math.max(1, Math.floor(Number(colorIndex)) || 1));
  return { color: `var(--collab-${n})`, colorLight: `color-mix(in srgb, var(--collab-${n}) 25%, transparent)` };
}

/** Names and colors of remote carets are rebuilt here; nothing a peer sends ends up in CSS. */
export function sanitizeState(state: unknown): unknown {
  if (!state || typeof state !== "object") return state;
  const s = state as { user?: { name?: unknown; colorIndex?: unknown } };
  if (!s.user) return state;
  const colorIndex = Math.min(8, Math.max(1, Math.floor(Number(s.user.colorIndex)) || 1));
  return { ...s, user: { name: String(s.user.name ?? t("tools.codeWithMe.guestName")).slice(0, 64), colorIndex, ...userStyle(colorIndex) } };
}

/** Create the local replica of a document the session opened for us. */
export function adoptDoc(tabPath: string, opened: DocOpened, me: Participant | undefined): SharedDoc {
  const existing = byId.get(opened.docId);
  if (existing) {
    byTab.delete(existing.tabPath);
    existing.tabPath = tabPath;
    byTab.set(tabPath, existing);
    return existing;
  }
  const ydoc = new Y.Doc();
  Y.applyUpdate(ydoc, opened.update, REMOTE);
  const ytext = ydoc.getText("content");
  const awareness = new Awareness(ydoc);
  if (me) awareness.setLocalStateField("user", { name: me.name, colorIndex: me.color, ...userStyle(me.color) });
  const undo = new Y.UndoManager(ytext, { trackedOrigins: new Set() });
  const docId = opened.docId;
  const onUpdate = (update: Uint8Array, origin: unknown) => {
    if (origin !== REMOTE) void collabApi.docUpdate(docId, update).catch((e) => console.warn("shared edit not sent", e));
  };
  const onAwareness = ({ added, updated, removed }: { added: number[]; updated: number[]; removed: number[] }, origin: unknown) => {
    if (origin === REMOTE) return;
    const mine = [...added, ...updated, ...removed].filter((c) => c === ydoc.clientID);
    if (mine.length) void collabApi.awareness(docId, encodeAwarenessUpdate(awareness, mine)).catch(() => undefined);
  };
  ydoc.on("update", onUpdate);
  awareness.on("update", onAwareness);
  const doc: SharedDoc = {
    docId,
    tabPath,
    ydoc,
    ytext,
    awareness,
    undo,
    savedHash: opened.saved.hash,
    readOnly: opened.readOnly,
    view: opened.view,
    dispose: () => {
      removeAwarenessStates(awareness, [ydoc.clientID], "local");
      ydoc.off("update", onUpdate);
      awareness.off("update", onAwareness);
      awareness.destroy();
      undo.destroy();
      ydoc.destroy();
    },
  };
  byId.set(docId, doc);
  byTab.set(tabPath, doc);
  for (const e of early.get(docId) ?? []) applyEvent(doc, e);
  early.delete(docId);
  return doc;
}

/** The document moved to another tab path (a rename on the host). */
export function retabDoc(doc: SharedDoc, tabPath: string): void {
  byTab.delete(doc.tabPath);
  doc.tabPath = tabPath;
  byTab.set(tabPath, doc);
}

/** The tab closed: tell the session and drop the replica. */
export function releaseDoc(tabPath: string, notify = true): void {
  const doc = byTab.get(tabPath);
  if (!doc) return;
  byTab.delete(tabPath);
  byId.delete(doc.docId);
  if (notify) void collabApi.docClose(doc.docId).catch(() => undefined);
  doc.dispose();
}

/** Drop every replica (the session ended). */
export function releaseAll(): void {
  for (const tab of [...byTab.keys()]) releaseDoc(tab, false);
  early.clear();
}

/** The guest whose edit an update is (host), for the gutter markers (`guestChanges.ts`). */
function authorOf(from: number | undefined): Author | null {
  if (from === undefined) return null;
  const p = session().participants.find((x) => x.id === from);
  return { color: p?.color ?? 1, name: p?.name ?? t("tools.codeWithMe.aGuest") };
}

function applyEvent(doc: SharedDoc, e: DocEvent): void {
  if (e.update) {
    const update = fromBase64(e.update);
    withAuthor(authorOf(e.from), () => Y.applyUpdate(doc.ydoc, update, REMOTE));
  }
  if (e.awareness) {
    const update = modifyAwarenessUpdate(fromBase64(e.awareness), sanitizeState);
    applyAwarenessUpdate(doc.awareness, update, REMOTE);
  }
  if (e.saved) doc.savedHash = e.saved.hash;
  if (e.saved || e.renamed !== undefined || e.closed !== undefined) listeners.forEach((l) => l(doc, e));
}

/** What resynchronizing a document needs from the session (`collabApi` in the app). */
export interface ResyncApi {
  docSync: (docId: number, stateVector: Uint8Array) => Promise<DocSynced>;
  docUpdate: (docId: number, update: Uint8Array) => Promise<void>;
}

/**
 * Bring one document up to date after the connection to the host was restored (Yjs sync
 * both ways): apply what the host has and this replica missed, then send the host the edits
 * made here while the connection was down. A rename or close while away arrives as the
 * matching document event; returns false when the document is gone.
 */
export async function resyncDoc(doc: SharedDoc, api: ResyncApi = collabApi): Promise<boolean> {
  let synced: DocSynced;
  try {
    synced = await api.docSync(doc.docId, Y.encodeStateVector(doc.ydoc));
  } catch (e) {
    handleDocEvent({ docId: doc.docId, closed: String((e as Error)?.message ?? e) });
    return false;
  }
  Y.applyUpdate(doc.ydoc, synced.update, REMOTE);
  const missing = Y.encodeStateAsUpdate(doc.ydoc, synced.stateVector);
  // An empty update is two zero bytes (no structs, no deletes).
  if (missing.length > 2) await api.docUpdate(doc.docId, missing);
  doc.readOnly = synced.readOnly;
  doc.view = synced.view;
  const renamed = doc.tabPath.startsWith(GUEST_PREFIX) && synced.path && GUEST_PREFIX + synced.path !== doc.tabPath;
  // A conflict may have opened or ended while the connection was down.
  handleDocEvent({ docId: doc.docId, saved: synced.saved, conflict: { open: !!synced.conflict, path: "" }, ...(renamed ? { renamed: synced.path } : {}) });
  return true;
}

/** Resynchronize every shared document of this window (a guest that reconnected). */
export async function resyncAll(api: ResyncApi = collabApi): Promise<void> {
  for (const doc of sharedDocs()) await resyncDoc(doc, api).catch((e) => console.warn("cannot resync a shared file", e));
}

/** A `collab://doc` event. */
export function handleDocEvent(e: DocEvent): void {
  if (e.conflict) conflictListener?.(e);
  const doc = byId.get(e.docId);
  if (doc) applyEvent(doc, e);
  else if (e.update || e.awareness) {
    const list = early.get(e.docId) ?? [];
    list.push(e);
    early.set(e.docId, list.slice(-200));
  }
}
