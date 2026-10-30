import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { clearGuestLines, guestChanges, markedLines, withAuthor } from "./guestChanges";

const alex = { color: 2, name: "Alex" };
const sam = { color: 3, name: "Sam" };

const start = () => EditorState.create({ doc: "one\ntwo\nthree\nfour\n", extensions: guestChanges() });

describe("lines changed by guests", () => {
  it("marks the lines a guest's update touched, not the host's own typing", () => {
    let s = start();
    s = s.update({ changes: { from: 0, insert: "host " } }).state;
    expect(markedLines(s)).toEqual([]);
    // Alex types on line 2 and pastes two lines after line 3.
    s = withAuthor(alex, () => s.update({ changes: [{ from: s.doc.line(2).to, insert: "!" }, { from: s.doc.line(3).to, insert: "\nx\ny" }] }).state);
    expect(markedLines(s)).toEqual([
      { line: 2, color: 2, name: "Alex" },
      { line: 3, color: 2, name: "Alex" },
      { line: 4, color: 2, name: "Alex" },
      { line: 5, color: 2, name: "Alex" },
    ]);
  });

  it("follows later edits, keeps one marker per line and the latest author", () => {
    let s = start();
    s = withAuthor(alex, () => s.update({ changes: { from: s.doc.line(3).from, insert: "a" } }).state);
    // The host adds two lines at the top: the marker moves with line 3.
    s = s.update({ changes: { from: 0, insert: "x\ny\n" } }).state;
    expect(markedLines(s).map((m) => m.line)).toEqual([5]);
    // Sam changes the same line: one marker in Sam's color.
    s = withAuthor(sam, () => s.update({ changes: { from: s.doc.line(5).to, insert: "b" } }).state);
    expect(markedLines(s)).toEqual([{ line: 5, color: 3, name: "Sam" }]);
    // The host joins lines 4 and 5: still one marker on the joined line.
    s = s.update({ changes: { from: s.doc.line(4).to, to: s.doc.line(5).from } }).state;
    expect(markedLines(s).map((m) => m.line)).toEqual([4]);
  });

  it("a guest deleting lines marks the line where they were", () => {
    let s = start();
    s = withAuthor(alex, () => s.update({ changes: { from: s.doc.line(2).from, to: s.doc.line(4).from } }).state);
    expect(s.doc.toString()).toBe("one\nfour\n");
    expect(markedLines(s).map((m) => m.line)).toEqual([2]);
  });

  it("saving clears the markers", () => {
    let s = start();
    s = withAuthor(alex, () => s.update({ changes: { from: 0, insert: "a" } }).state);
    expect(markedLines(s)).toHaveLength(1);
    s = s.update({ effects: clearGuestLines.of(null) }).state;
    expect(markedLines(s)).toEqual([]);
  });

  it("restores the previous author after nested or failing calls", () => {
    let s = start();
    expect(() =>
      withAuthor(alex, () => {
        throw new Error("bad update");
      }),
    ).toThrow();
    s = s.update({ changes: { from: 0, insert: "host" } }).state;
    expect(markedLines(s)).toEqual([]);
  });
});
