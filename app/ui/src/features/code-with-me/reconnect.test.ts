import { afterEach, describe, expect, it, vi } from "vitest";
import * as Y from "yjs";
import { collabApi, type DocOpened, type DocSynced } from "@/ipc/collab";
import { contentHash } from "@/state/editor";
import { adoptDoc, GUEST_PREFIX, onSharedDocEvent, releaseAll, resyncDoc, setConflictListener } from "./docs";
import { catchUpAfterResume, connectionLabel } from "./reconnect";

const PATH = "app/Order.php";

afterEach(() => {
  releaseAll();
  vi.restoreAllMocks();
});

/** The host's replica and a guest's window whose connection is down. */
function setup(text = "hello world") {
  const host = new Y.Doc();
  host.getText("content").insert(0, text);
  // Nothing reaches the host while the connection is down.
  vi.spyOn(collabApi, "docUpdate").mockResolvedValue(undefined);
  vi.spyOn(collabApi, "awareness").mockResolvedValue(undefined);
  vi.spyOn(collabApi, "docClose").mockResolvedValue(undefined);
  const opened: DocOpened = { docId: 3, path: PATH, update: Y.encodeStateAsUpdate(host), saved: { hash: contentHash(text) }, readOnly: false };
  const doc = adoptDoc(GUEST_PREFIX + PATH, opened, undefined);
  /** The host's side of `collab_doc_sync` and the update that follows. */
  const api = {
    docSync: async (docId: number, sv: Uint8Array): Promise<DocSynced> => ({
      docId,
      path: PATH,
      update: Y.encodeStateAsUpdate(host, sv),
      stateVector: Y.encodeStateVector(host),
      saved: { hash: "saved" },
      readOnly: false,
    }),
    docUpdate: vi.fn(async (_id: number, update: Uint8Array) => Y.applyUpdate(host, update)),
  };
  return { host, doc, api };
}

describe("a guest that reconnected", () => {
  it("gets the host's edits and sends the ones it made while the connection was down", async () => {
    const { host, doc, api } = setup();
    doc.ytext.insert(0, ">> ");
    host.getText("content").insert(11, "!");
    expect(await resyncDoc(doc, api)).toBe(true);
    expect(doc.ytext.toString()).toBe(">> hello world!");
    expect(host.getText("content").toString()).toBe(">> hello world!");
    expect(doc.savedHash).toBe("saved");
  });

  it("sends nothing when it made no edits", async () => {
    const { host, doc, api } = setup();
    host.getText("content").insert(0, "x");
    await resyncDoc(doc, api);
    expect(api.docUpdate).not.toHaveBeenCalled();
    expect(doc.ytext.toString()).toBe("xhello world");
  });

  it("follows a rename and closes a file that is gone", async () => {
    const { doc, api } = setup();
    const events: string[] = [];
    onSharedDocEvent((_d, e) => events.push(e.renamed ?? e.closed ?? "saved"));
    await resyncDoc(doc, { ...api, docSync: async (id, sv) => ({ ...(await api.docSync(id, sv)), path: "app/Renamed.php" }) });
    expect(events).toContain("app/Renamed.php");
    const gone = await resyncDoc(doc, { ...api, docSync: () => Promise.reject(new Error("The file is no longer shared.")) });
    expect(gone).toBe(false);
    expect(events).toContain("The file is no longer shared.");
  });

  it("learns whether a File Cache Conflict opened or ended meanwhile", async () => {
    const { doc, api } = setup();
    const conflicts: boolean[] = [];
    setConflictListener((e) => conflicts.push(!!e.conflict?.open));
    await resyncDoc(doc, { ...api, docSync: async (id, sv) => ({ ...(await api.docSync(id, sv)), conflict: true }) });
    await resyncDoc(doc, api);
    expect(conflicts).toEqual([true, false]);
    setConflictListener(() => undefined);
  });
});

describe("the connection label", () => {
  it("says Reconnecting… with the next attempt", () => {
    expect(connectionLabel(undefined)).toBeNull();
    expect(connectionLabel({ state: "connected" })).toBeNull();
    expect(connectionLabel({ state: "reconnecting", attempt: 2, nextRetrySecs: 4 })).toBe("Reconnecting… (next attempt in 4 s)");
    expect(connectionLabel({ state: "reconnecting", attempt: 1, nextRetrySecs: 0 })).toBe("Reconnecting…");
  });
});

describe("catching up after a resume", () => {
  it("re-reads the project tree and refreshes the views that follow file changes", () => {
    const refreshTree = vi.fn();
    const dispatch = vi.fn();
    catchUpAfterResume({ refreshTree, dispatch });
    expect(refreshTree).toHaveBeenCalledOnce();
    expect(dispatch).toHaveBeenCalledWith({ changed: [], removed: [], git: true });
  });
});
