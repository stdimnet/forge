import { afterEach, describe, expect, it, vi } from "vitest";
import * as Y from "yjs";
import { Awareness, encodeAwarenessUpdate } from "y-protocols/awareness";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { history } from "@codemirror/commands";
import { collabApi, toBase64, type DocEvent, type DocOpened } from "@/ipc/collab";
import { extensionsFor } from "@/editor/extensions";
import { historyCompartment, redoIn, undoIn } from "@/editor/reload";
import { contentHash, isDirty, type EditorTab } from "@/state/editor";
import { adoptDoc, handleDocEvent, onSharedDocEvent, releaseAll, sanitizeState, sharedDoc, userStyle } from "./docs";
import "./binding";

const PATH = "app/Order.php";
const ME = { id: 1, name: "Robert", role: "guest" as const, color: 2, permissions: { files: "edit", terminal: "none", run: "none", vcs: "readOnly" } } as const;

/** A participant's window, connected to a "host" replica through a fake IPC. */
function connect(text = "hello world") {
  const host = new Y.Doc();
  host.getText("content").insert(0, text);
  const sent: Uint8Array[] = [];
  vi.spyOn(collabApi, "docUpdate").mockImplementation(async (_id, update) => {
    sent.push(update);
    Y.applyUpdate(host, update, "from-guest");
  });
  vi.spyOn(collabApi, "awareness").mockResolvedValue(undefined);
  vi.spyOn(collabApi, "docClose").mockResolvedValue(undefined);
  const opened: DocOpened = { docId: 7, path: PATH, update: Y.encodeStateAsUpdate(host), saved: { hash: contentHash(text) }, readOnly: false };
  const doc = adoptDoc(PATH, opened, ME);
  const view = new EditorView({
    parent: document.body,
    state: EditorState.create({ doc: text, extensions: [historyCompartment.of(history()), ...extensionsFor(PATH)] }),
  });
  /** What the host does when one of its replicas changes: forward it to this window. */
  const deliver = (fn: () => void) => {
    const before = Y.encodeStateVector(host);
    fn();
    const update = Y.encodeStateAsUpdate(host, before);
    handleDocEvent({ docId: 7, update: toBase64(update) });
  };
  return { host, hostText: host.getText("content"), doc, view, sent, deliver };
}

const settle = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  releaseAll();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("shared editor binding", () => {
  it("shows the host's text and sends local edits as Yjs updates", async () => {
    const { view, host, sent } = connect();
    await settle();
    expect(view.state.doc.toString()).toBe("hello world");
    view.dispatch({ changes: { from: 5, insert: ", dear" }, userEvent: "input.type" });
    expect(host.getText("content").toString()).toBe("hello, dear world");
    expect(sent).toHaveLength(1);
    view.destroy();
  });

  it("applies remote edits to the view without sending them back", async () => {
    const { view, hostText, deliver, sent } = connect();
    await settle();
    deliver(() => hostText.insert(11, "!"));
    expect(view.state.doc.toString()).toBe("hello world!");
    expect(sent).toHaveLength(0);
    view.destroy();
  });

  it("undoes only the participant's own edits", async () => {
    const { view, hostText, deliver, host } = connect();
    await settle();
    view.dispatch({ changes: { from: 0, insert: "A: " }, userEvent: "input.type" });
    deliver(() => hostText.insert(hostText.length, " [B]"));
    expect(view.state.doc.toString()).toBe("A: hello world [B]");
    expect(undoIn(view)).toBe(true);
    expect(view.state.doc.toString()).toBe("hello world [B]");
    expect(host.getText("content").toString()).toBe("hello world [B]");
    expect(redoIn(view)).toBe(true);
    expect(view.state.doc.toString()).toBe("A: hello world [B]");
    view.destroy();
  });

  it("does not undo anything when the participant typed nothing", async () => {
    const { view, hostText, deliver } = connect();
    await settle();
    deliver(() => hostText.insert(0, "B "));
    undoIn(view);
    expect(view.state.doc.toString()).toBe("B hello world");
    view.destroy();
  });

  it("ignores a disk change's origin in the undo stack", async () => {
    const { view, hostText, deliver } = connect();
    await settle();
    view.dispatch({ changes: { from: 0, insert: "x" }, userEvent: "input.type" });
    deliver(() => hostText.delete(1, 5));
    undoIn(view);
    expect(view.state.doc.toString()).toBe(" world");
    view.destroy();
  });

  it("drops the binding when the tab is released", async () => {
    const { view } = connect();
    await settle();
    expect(sharedDoc(PATH)).toBeDefined();
    releaseAll();
    expect(sharedDoc(PATH)).toBeUndefined();
    view.destroy();
  });
});

describe("shared document events", () => {
  it("keeps events that arrive before the document opens", () => {
    const host = new Y.Doc();
    host.getText("content").insert(0, "abc");
    const base = Y.encodeStateAsUpdate(host);
    const before = Y.encodeStateVector(host);
    host.getText("content").insert(3, "d");
    handleDocEvent({ docId: 9, update: toBase64(Y.encodeStateAsUpdate(host, before)) });
    const doc = adoptDoc("early.php", { docId: 9, path: "early.php", update: base, saved: { hash: "x" }, readOnly: false }, ME);
    expect(doc.ytext.toString()).toBe("abcd");
  });

  it("reports saves, renames and closes", () => {
    const { doc } = connect();
    const seen: DocEvent[] = [];
    const off = onSharedDocEvent((d, e) => d === doc && seen.push(e));
    handleDocEvent({ docId: 7, saved: { hash: "9:abc" } });
    handleDocEvent({ docId: 7, renamed: "app/Other.php" });
    expect(doc.savedHash).toBe("9:abc");
    expect(seen.map((e) => Object.keys(e).filter((k) => k !== "docId")[0])).toEqual(["saved", "renamed"]);
    off();
  });

  it("marks a tab dirty when its text differs from the saved hash", () => {
    const tab = { path: PATH, name: "Order.php", content: "abc", saved: "abc", eol: "\n", encoding: "UTF-8", bom: false, cursor: { line: 1, col: 1 }, shared: true, savedHash: contentHash("abc") } satisfies EditorTab;
    expect(isDirty(tab)).toBe(false);
    expect(isDirty({ ...tab, content: "abcd" })).toBe(true);
    expect(isDirty({ ...tab, content: "abcd", savedHash: contentHash("abcd") })).toBe(false);
  });
});

describe("participant presence", () => {
  it("rebuilds names and colors from the theme, never from the peer", () => {
    const state = sanitizeState({ user: { name: "x".repeat(200), colorIndex: 99, color: "red; background:url(//evil)", colorLight: "x" }, cursor: null }) as {
      user: { name: string; colorIndex: number; color: string; colorLight: string };
    };
    expect(state.user.name).toHaveLength(64);
    expect(state.user.colorIndex).toBe(8);
    expect(state.user.color).toBe("var(--collab-8)");
    expect(state.user.colorLight).toContain("var(--collab-8)");
    expect(userStyle(0).color).toBe("var(--collab-1)");
    expect(userStyle(Number.NaN).color).toBe("var(--collab-1)");
  });

  it("sanitizes awareness that arrives from the session", () => {
    const { doc } = connect();
    const peer = new Awareness(new Y.Doc());
    peer.setLocalStateField("user", { name: "Alex", colorIndex: 3, color: "url(//evil)", colorLight: "url(//evil)" });
    handleDocEvent({ docId: 7, awareness: toBase64(encodeAwarenessUpdate(peer, [peer.clientID])) });
    const remote = doc.awareness.getStates().get(peer.clientID) as { user: { color: string } } | undefined;
    expect(remote?.user.color).toBe("var(--collab-3)");
    peer.destroy();
  });

  it("starts with the participant's own name and color", () => {
    const { doc } = connect();
    const mine = doc.awareness.getLocalState() as { user: { name: string; color: string } };
    expect(mine.user.name).toBe("Robert");
    expect(mine.user.color).toBe("var(--collab-2)");
  });
});
