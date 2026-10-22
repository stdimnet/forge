import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { history, undo } from "@codemirror/commands";
import { callWasm, forgeWasm } from "@/wasm";
import type { MergeModel } from "./api";
import {
  applyNonConflicting,
  applySide,
  countsLabel,
  ignoreSide,
  lineOffsets,
  mapLine,
  mergeCounts,
  mergeResultState,
  nextChunk,
  resolveSimpleConflicts,
  resultLines,
} from "./mergeState";

const base = "<?php\nclass Cart\n{\n    $a = $price * $qty;\n}\n// end\n";
// Left: renames the class and edits a word of line 4; right: other words of 2 and 4, adds a line.
const left = "<?php\nclass ShoppingCart\n{\n    $a = $netPrice * $qty;\n}\n// end\n";
const right = "<?php\nclass Basket\n{\n    $a = $price * $quantity;\n}\n// end\n// more\n";

async function setup() {
  const w = await forgeWasm();
  const model = await callWasm<MergeModel>(w.vcs_merge, { base, left, right });
  const { extension, field } = mergeResultState(model.chunks, base);
  let state = EditorState.create({ doc: base, extensions: [extension, history()] });
  const run = (spec: ReturnType<typeof applySide>) => {
    expect(spec).not.toBeNull();
    state = state.update(spec!).state;
  };
  const counts = () => mergeCounts(state.field(field), model.chunks);
  return { model, field, get state() { return state; }, set state(s) { state = s; }, run, counts };
}

describe("merge result state", () => {
  it("computes line offsets", () => {
    expect(lineOffsets("a\nb\n")).toEqual([0, 2, 4]);
    expect(lineOffsets("a\nb")).toEqual([0, 2, 3]);
    expect(lineOffsets("")).toEqual([0]);
  });

  it("starts from the base with every change unresolved", async () => {
    const m = await setup();
    expect(m.model.chunks.map((c) => c.kind)).toEqual(["conflict", "conflict", "right"]);
    expect(m.counts()).toMatchObject({ changes: 1, conflicts: 2, simple: 1, left: 0, right: 1 });
    expect(countsLabel(m.counts())).toBe("1 change. 2 conflicts");
    expect(m.state.doc.toString()).toBe(base);
  });

  it("applies non-conflicting changes and simple conflicts", async () => {
    const m = await setup();
    expect(applyNonConflicting(m.state, m.field, m.model.chunks, "left")).toBeNull();
    m.run(applyNonConflicting(m.state, m.field, m.model.chunks, "all"));
    expect(m.state.doc.toString()).toBe(`${base}// more\n`);
    m.run(resolveSimpleConflicts(m.state, m.field, m.model.chunks));
    expect(m.state.doc.toString()).toContain("$a = $netPrice * $quantity;");
    expect(m.counts()).toMatchObject({ changes: 0, conflicts: 1, simple: 0 });
  });

  it("accepts one side of a conflict and appends the other", async () => {
    const m = await setup();
    m.run(applySide(m.state, m.field, m.model.chunks, 0, "left"));
    expect(m.state.doc.line(2).text).toBe("class ShoppingCart");
    expect(m.counts().conflicts).toBe(2);
    m.run(applySide(m.state, m.field, m.model.chunks, 0, "right"));
    expect(m.state.doc.toString()).toContain("class ShoppingCart\nclass Basket\n{");
    expect(m.counts().conflicts).toBe(1);
    // Undo restores text and state.
    m.state = undone(m.state);
    expect(m.state.doc.toString()).not.toContain("class Basket");
    expect(m.counts().conflicts).toBe(2);
  });

  it("ignores sides", async () => {
    const m = await setup();
    m.run(ignoreSide(m.state, m.field, m.model.chunks, 2, "right"));
    expect(m.counts().changes).toBe(0);
    m.run(ignoreSide(m.state, m.field, m.model.chunks, 0, "left"));
    expect(m.counts().conflicts).toBe(2);
    m.run(applySide(m.state, m.field, m.model.chunks, 0, "right"));
    expect(m.state.doc.line(2).text).toBe("class Basket");
    expect(m.counts().conflicts).toBe(1);
  });

  it("keeps chunk positions through edits", async () => {
    const m = await setup();
    m.state = m.state.update({ changes: { from: 0, insert: "// header\n" } }).state;
    const chunk = m.state.field(m.field)[0];
    expect(m.state.sliceDoc(chunk.from, chunk.to)).toBe("class Cart\n");
    expect(resultLines(m.state, chunk.from, chunk.to)).toEqual([2, 3]);
    m.run(applySide(m.state, m.field, m.model.chunks, 0, "right"));
    expect(m.state.doc.line(3).text).toBe("class Basket");
  });

  it("navigates between unresolved chunks", async () => {
    const m = await setup();
    const list = m.state.field(m.field);
    expect(nextChunk(list, 0, true)).toBe(0);
    expect(nextChunk(list, list[0].from, true)).toBe(1);
    expect(nextChunk(list, list[0].from, false)).toBe(2);
    expect(nextChunk(list.map((c) => ({ ...c, left: true, right: true })), 0, true)).toBe(-1);
  });

  it("maps lines between panes", () => {
    const anchors: [number, number, number, number][] = [
      [2, 3, 2, 5],
      [10, 10, 12, 14],
    ];
    expect(mapLine(1, anchors)).toBe(1);
    expect(mapLine(2, anchors)).toBe(2);
    expect(mapLine(4, anchors)).toBe(6);
    expect(mapLine(11, anchors)).toBe(15);
  });
});

function undone(state: EditorState): EditorState {
  let next = state;
  undo({ state, dispatch: (tr) => (next = tr.state) });
  return next;
}
