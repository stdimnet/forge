import { afterEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { redo, undo } from "@codemirror/commands";
import { replaceDocument, undoHistory } from "./reload";

const views: EditorView[] = [];
const make = (doc: string) => {
  const view = new EditorView({ state: EditorState.create({ doc, extensions: [undoHistory()] }) });
  views.push(view);
  return view;
};

// jsdom has no layout; destroy the views before CodeMirror's measure pass runs.
afterEach(() => views.splice(0).forEach((v) => v.destroy()));

describe("replaceDocument", () => {
  it("keeps normal edits undoable", () => {
    const view = make("a\nb\n");
    view.dispatch({ changes: { from: 4, insert: "c\n" } });
    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe("a\nb\n");
  });

  it("makes undo ignore everything before a reload", () => {
    const view = make("a\nb\n");
    view.dispatch({ changes: { from: 4, insert: "typed\n" } });
    replaceDocument(view, "a\nb\nfrom disk\n");
    expect(view.state.doc.toString()).toBe("a\nb\nfrom disk\n");
    expect(undo(view)).toBe(false);
    expect(redo(view)).toBe(false);
    expect(view.state.doc.toString()).toBe("a\nb\nfrom disk\n");
  });

  it("records edits made after the reload", () => {
    const view = make("one\n");
    replaceDocument(view, "one\ntwo\n");
    view.dispatch({ changes: { from: 8, insert: "three\n" } });
    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe("one\ntwo\n");
    expect(undo(view)).toBe(false);
  });

  it("leaves identical text alone and keeps the caret inside the new text", () => {
    const view = make("abc\ndef");
    view.dispatch({ selection: { anchor: 7 } });
    replaceDocument(view, "abc\ndef");
    expect(view.state.selection.main.head).toBe(7);
    replaceDocument(view, "ab");
    expect(view.state.selection.main.head).toBe(2);
  });
});
