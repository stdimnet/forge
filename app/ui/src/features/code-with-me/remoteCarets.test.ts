import { afterEach, describe, expect, it, vi } from "vitest";
import * as Y from "yjs";
import { Awareness, applyAwarenessUpdate, encodeAwarenessUpdate } from "y-protocols/awareness";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { stripeMarks } from "@/editor/errorStripe";
import { activeLabels, caretMarks, LABEL_MS, remoteCarets } from "./remoteCarets";

const TEXT = "<?php\necho 'one';\necho 'two';\n";

/** A local replica and a remote participant ("Alex", color 2) on the same text. */
function pair() {
  const local = new Y.Doc();
  local.getText("content").insert(0, TEXT);
  const remote = new Y.Doc();
  Y.applyUpdate(remote, Y.encodeStateAsUpdate(local));
  const awareness = new Awareness(local);
  const remoteAwareness = new Awareness(remote);
  const moveTo = (index: number) => {
    const pos = Y.createRelativePositionFromTypeIndex(remote.getText("content"), index);
    remoteAwareness.setLocalState({ user: { name: "Alex", colorIndex: 2 }, cursor: { anchor: pos, head: pos } });
    applyAwarenessUpdate(awareness, encodeAwarenessUpdate(remoteAwareness, [remote.clientID]), "remote");
  };
  return { local, awareness, moveTo };
}

const flush = () => new Promise<void>((r) => queueMicrotask(r));

afterEach(() => vi.useRealTimers());

describe("remote carets", () => {
  it("are stripe marks in the participant's color", () => {
    const { local, awareness, moveTo } = pair();
    expect(caretMarks(awareness, local.getText("content"))).toEqual([]);
    moveTo(TEXT.indexOf("two"));
    expect(caretMarks(awareness, local.getText("content"))).toEqual([{ pos: TEXT.indexOf("two"), cls: "cm-stripe-collab-2", title: "Alex" }]);
  });

  it("show their name only while they move", () => {
    expect(activeLabels(new Map([[2, 1000], [3, 0]]), 1000 + LABEL_MS - 1)).toEqual([2]);
    expect(activeLabels(new Map([[2, 1000]]), 1000 + LABEL_MS)).toEqual([]);
  });

  it("mark the editor and fade the label after two seconds", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
    const { local, awareness, moveTo } = pair();
    const view = new EditorView({
      state: EditorState.create({ doc: TEXT, extensions: remoteCarets(awareness, local.getText("content")) }),
      parent: document.createElement("div"),
    });
    moveTo(TEXT.indexOf("one"));
    await flush();
    expect(view.dom.classList.contains("cwm-label-2")).toBe(true);
    expect(view.state.facet(stripeMarks)).toEqual([{ pos: TEXT.indexOf("one"), cls: "cm-stripe-collab-2", title: "Alex" }]);

    vi.advanceTimersByTime(LABEL_MS - 500);
    moveTo(TEXT.indexOf("two"));
    await flush();
    vi.advanceTimersByTime(LABEL_MS - 100);
    // Moving again keeps the label.
    expect(view.dom.classList.contains("cwm-label-2")).toBe(true);
    expect(view.state.facet(stripeMarks)[0].pos).toBe(TEXT.indexOf("two"));
    vi.advanceTimersByTime(200);
    expect(view.dom.classList.contains("cwm-label-2")).toBe(false);
    // The stripe mark stays where the caret is.
    expect(view.state.facet(stripeMarks)).toHaveLength(1);

    // Local edits move the mark with the text.
    view.dispatch({ changes: { from: 0, insert: "// x\n" } });
    expect(view.state.facet(stripeMarks)[0].pos).toBe(TEXT.indexOf("two") + 5);
    view.destroy();
  });
});
