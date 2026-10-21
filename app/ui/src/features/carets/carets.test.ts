import { describe, expect, it } from "vitest";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView, runScopeHandlers } from "@codemirror/view";
import { blockSelection, caretExtensions, caretsAtLineEnds, cloneCarets, extendBlock, isColumnMode, selectWordsAtCarets, setColumnMode, toggleCaretAt } from "./carets";
import { copiedFrom, needsCaretPaste, pasteSpec, recordCopy } from "@/features/clipboard/clipboard";
import { clipboardApi } from "@/ipc/clipboard";
import "@/mock/clipboardMock";

// jsdom has no layout; CodeMirror measures text ranges after updates.
const noRects = () => Object.assign([], { item: () => null }) as unknown as DOMRectList;
Range.prototype.getClientRects ??= noRects;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

/** A state whose carets are at every `|`. */
function stateOf(doc: string, extensions = [EditorState.allowMultipleSelections.of(true)]): EditorState {
  const parts = doc.split("|");
  const carets: number[] = [];
  let text = parts[0];
  for (const part of parts.slice(1)) {
    carets.push(text.length);
    text += part;
  }
  return EditorState.create({
    doc: text,
    selection: EditorSelection.create((carets.length ? carets : [0]).map((p) => EditorSelection.cursor(p))),
    extensions,
  });
}

/** The document with `|` at every caret and `[…]` around selections. */
function shown(state: EditorState): string {
  let out = state.doc.toString();
  for (const r of [...state.selection.ranges].reverse()) {
    out = r.empty ? `${out.slice(0, r.from)}|${out.slice(r.from)}` : `${out.slice(0, r.from)}[${out.slice(r.from, r.to)}]${out.slice(r.to)}`;
  }
  return out;
}

const withSelection = (state: EditorState, selection: EditorSelection) => state.update({ selection }).state;

describe("Clone Caret Above / Below", () => {
  it("adds a caret on the next line at the same column, clamped to short lines", () => {
    let state = stateOf("abcdef\nab|cdef\nx\nabcdef");
    let out = cloneCarets(state, 1)!;
    state = withSelection(state, out.selection);
    expect(shown(state)).toBe("abcdef\nab|cdef\nx|\nabcdef");
    // The column is remembered past the short line.
    out = cloneCarets(state, 1, out.history)!;
    state = withSelection(state, out.selection);
    expect(shown(state)).toBe("abcdef\nab|cdef\nx|\nab|cdef");
    expect(state.selection.main.head).toBe(state.doc.line(4).from + 2);
    expect(cloneCarets(state, 1, out.history)).toBeNull();
  });

  it("takes back the last clones when the direction changes", () => {
    let state = stateOf("aaa\na|aa\naaa\naaa");
    let out = cloneCarets(state, 1)!;
    state = withSelection(state, out.selection);
    out = cloneCarets(state, 1, out.history)!;
    state = withSelection(state, out.selection);
    expect(state.selection.ranges.length).toBe(3);
    out = cloneCarets(state, -1, out.history)!;
    state = withSelection(state, out.selection);
    expect(shown(state)).toBe("aaa\na|aa\na|aa\naaa");
    out = cloneCarets(state, -1, out.history)!;
    state = withSelection(state, out.selection);
    expect(shown(state)).toBe("aaa\na|aa\naaa\naaa");
    // Now a new clone upwards.
    out = cloneCarets(state, -1, out.history)!;
    expect(shown(withSelection(state, out.selection))).toBe("a|aa\na|aa\naaa\naaa");
  });
});

describe("multiple carets", () => {
  it("adds carets to the ends of selected lines", () => {
    const state = EditorState.create({
      doc: "one\ntwo\nthree\nfour",
      selection: EditorSelection.single(1, 14),
      extensions: [EditorState.allowMultipleSelections.of(true)],
    });
    expect(shown(withSelection(state, caretsAtLineEnds(state)!))).toBe("one|\ntwo|\nthree|\nfour");
    expect(caretsAtLineEnds(stateOf("a|b"))).toBeNull();
  });

  it("adds a caret on Alt+Shift+Click and removes the clicked one", () => {
    const state = stateOf("ab|cd");
    let sel = toggleCaretAt(state.selection, 4);
    expect(sel.ranges.length).toBe(2);
    sel = toggleCaretAt(sel, 2);
    expect(sel.ranges.map((r) => r.head)).toEqual([4]);
    // The last caret stays.
    expect(toggleCaretAt(sel, 4).ranges.length).toBe(1);
  });

  it("selects the word at every caret for Select All Occurrences", () => {
    const state = stateOf("$us|er = $user;");
    expect(shown(withSelection(state, selectWordsAtCarets(state)!))).toBe("$[user] = $user;");
  });
});

describe("Column Selection Mode", () => {
  it("builds one range per line with short lines at their end", () => {
    const state = stateOf("abcdef\nab\nabcdef");
    const block = extendBlock(state, { anchorLine: 1, anchorCol: 1, headLine: 1, headCol: 1 }, 2, 3);
    expect(shown(withSelection(state, blockSelection(state, block)))).toBe("a[bcd]ef\na[b]\na[bcd]ef");
  });

  it("extends the rectangle with Shift+arrows only while the mode is on", () => {
    const parent = document.createElement("div");
    const view = new EditorView({ parent, state: stateOf("abc|def\nabcdef\nabcdef", [EditorState.allowMultipleSelections.of(true), caretExtensions()]) });
    const press = (key: string) => runScopeHandlers(view, new KeyboardEvent("keydown", { key, shiftKey: true }), "editor");
    expect(press("ArrowDown")).toBe(false);
    setColumnMode(view, true);
    expect(isColumnMode(view.state)).toBe(true);
    press("ArrowDown");
    press("ArrowDown");
    press("ArrowRight");
    press("ArrowRight");
    expect(shown(view.state)).toBe("abc[de]f\nabc[de]f\nabc[de]f");
    press("ArrowUp");
    expect(shown(view.state)).toBe("abc[de]f\nabc[de]f\nabcdef");
    view.destroy();
  });
});

describe("copy and paste with multiple carets", () => {
  it("copies every selection, or whole lines without one", () => {
    const state = withSelection(stateOf("one two\nthree"), EditorSelection.create([EditorSelection.range(0, 3), EditorSelection.range(8, 13)]));
    expect(copiedFrom(state)).toMatchObject({ text: "one\nthree", carets: ["one", "three"], linewise: false });
    expect(copiedFrom(stateOf("o|ne\ntwo"))).toMatchObject({ text: "one", linewise: true });
  });

  it("pastes one part per caret when the copy had as many carets", () => {
    const state = stateOf("|\n|");
    const parts = ["a\nb", "c"];
    const pasted = state.update(pasteSpec(state, "a\nb\nc", parts)).state;
    expect(shown(pasted)).toBe("a\nb|\nc|");
    // With another caret count the whole text goes to every caret.
    const one = stateOf("x|");
    expect(one.update(pasteSpec(one, "a\nb\nc", parts)).state.doc.toString()).toBe("xa\nb\nc");
    // Lines are spread when their count matches the carets.
    const two = stateOf("|-|");
    expect(two.update(pasteSpec(two, "1\r\n2")).state.doc.toString()).toBe("1-2");
  });

  it("takes over a native paste of the last multi-caret copy", () => {
    recordCopy("a\nb\nc", ["a\nb", "c"]);
    expect(needsCaretPaste(stateOf("|\n|"), "a\r\nb\r\nc")).toBe(true);
    expect(needsCaretPaste(stateOf("|"), "a\nb\nc")).toBe(false);
    expect(needsCaretPaste(stateOf("|\n|"), "other")).toBe(false);
  });

  it("keeps copies in the clipboard history, newest first", async () => {
    recordCopy("first");
    recordCopy("second");
    await new Promise((r) => setTimeout(r, 20));
    const items = await clipboardApi.list("", 20);
    expect(items[0].text).toBe("second");
    expect(items.map((i) => i.text)).toContain("first");
    expect((await clipboardApi.list("fir")).map((i) => i.text)).toEqual(["first"]);
  });
});
