import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { stopOnLocalActions } from "./followStop";

let view: EditorView | null = null;

function editor(active: () => boolean, stop: () => void): EditorView {
  const parent = document.createElement("div");
  document.body.append(parent);
  // jsdom has no layout: keep CodeMirror's own mouse selection out of the test.
  const noMouseSelection = EditorView.domEventHandlers({ mousedown: () => true });
  const extensions = [stopOnLocalActions(active, stop), noMouseSelection];
  view = new EditorView({ state: EditorState.create({ doc: "one\ntwo\nthree", extensions }), parent });
  return view;
}

afterEach(() => {
  view?.destroy();
  view = null;
  vi.useRealTimers();
});

describe("stopping to follow", () => {
  it("stops on typing and caret moves, only while following", () => {
    let following = true;
    const stop = vi.fn(() => (following = false));
    const v = editor(() => following, stop);
    v.dispatch({ selection: { anchor: 2 } });
    expect(stop).not.toHaveBeenCalled();
    v.dispatch({ changes: { from: 0, insert: "x" }, userEvent: "input.type" });
    expect(stop).toHaveBeenCalledTimes(1);
    v.dispatch({ selection: { anchor: 1 }, userEvent: "select" });
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("stops after a click only when the button is released, so the banner keeps its place meanwhile", () => {
    vi.useFakeTimers();
    let following = true;
    const stop = vi.fn(() => (following = false));
    const v = editor(() => following, stop);
    v.contentDOM.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0, clientX: 5, clientY: 5 }));
    // The click's selection does not stop following while the button is down.
    v.dispatch({ selection: { anchor: 5 }, userEvent: "select.pointer" });
    vi.runAllTimers();
    expect(stop).not.toHaveBeenCalled();
    window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, button: 0 }));
    expect(stop).not.toHaveBeenCalled();
    vi.runAllTimers();
    expect(stop).toHaveBeenCalledTimes(1);
    // Later keys stop again once following is back on.
    following = true;
    v.dispatch({ selection: { anchor: 1 }, userEvent: "select" });
    expect(stop).toHaveBeenCalledTimes(2);
  });
});
