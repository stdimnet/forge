// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

vi.mock("@/ipc", () => ({
  invoke: vi.fn(async () => null),
  listen: vi.fn(async () => () => {}),
  inTauri: false,
}));

import { setLocalSetting } from "@/state/settings";
import { inlineBlameEnabled, inlineBlameExtension, setBlameEffect } from "./editorGit";

const blame = {
  commits: [{ sha: "a".repeat(40), author: "Marlon", email: "m@example.com", time: 1_700_000_000, summary: "Add order totals", uncommitted: false }],
  lines: [0, 0],
};

function editor(): EditorView {
  const view = new EditorView({ state: EditorState.create({ doc: "one\ntwo", extensions: inlineBlameExtension }), parent: document.body });
  view.dispatch({ effects: setBlameEffect.of(blame as never) });
  return view;
}

const hint = (view: EditorView) => view.dom.querySelector(".cm-git-inline-blame");

describe("Show inline blame (vcs.git.inlineBlame)", () => {
  afterEach(() => {
    setLocalSetting("vcs.git.inlineBlame", true);
    vi.useRealTimers();
  });

  it("shows the hint on the caret line by default", () => {
    const view = editor();
    expect(inlineBlameEnabled()).toBe(true);
    expect(hint(view)?.textContent).toContain("Marlon");
    expect(hint(view)?.textContent).toContain("Add order totals");
    view.destroy();
  });

  it("shows no hint when the setting is off", () => {
    setLocalSetting("vcs.git.inlineBlame", false);
    const view = editor();
    expect(hint(view)).toBeNull();
    // Moving the caret does not bring it back.
    view.dispatch({ selection: { anchor: view.state.doc.line(2).from } });
    expect(hint(view)).toBeNull();
    view.destroy();
  });

  it("applies a changed setting to open editors", () => {
    vi.useFakeTimers();
    const view = editor();
    expect(hint(view)).not.toBeNull();
    setLocalSetting("vcs.git.inlineBlame", false);
    vi.runAllTimers();
    expect(hint(view)).toBeNull();
    setLocalSetting("vcs.git.inlineBlame", true);
    vi.runAllTimers();
    expect(hint(view)).not.toBeNull();
    view.destroy();
  });
});
