/**
 * Collapse / Expand (and their Recursively variants) act on the innermost region around the
 * caret, like PHPStorm (#218).
 */
import { afterEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { codeFolding, ensureSyntaxTree, foldedRanges } from "@codemirror/language";
import { php } from "@codemirror/lang-php";
import { collapseRegion, collapseRegionRecursively, enclosingFoldRange, expandRegion, expandRegionRecursively } from "./foldRegion";

const SOURCE = `<?php

class User extends Model
{
    public function orders(): HasMany
    {
        return $this->hasMany(Order::class);
    }

    public function totalSpent(): float
    {
        $sum = 0;
        foreach ($this->orders as $order) {
            $sum += $order->total;
        }
        return $sum;
    }
}
`;

let view: EditorView | undefined;
afterEach(() => {
  view?.destroy();
  view = undefined;
});

function editor(at: string, offset = 0): EditorView {
  const state = EditorState.create({ doc: SOURCE, extensions: [php(), codeFolding()] });
  ensureSyntaxTree(state, state.doc.length, 5000);
  const pos = SOURCE.indexOf(at) + offset;
  if (pos < offset) throw new Error(`${at} not found`);
  view = new EditorView({ state, parent: document.body });
  view.dispatch({ selection: { anchor: pos } });
  return view;
}

const folds = (v: EditorView) => {
  const out: string[] = [];
  foldedRanges(v.state).between(0, v.state.doc.length, (from, to) => {
    out.push(v.state.sliceDoc(from, to).trim().split("\n")[0]);
  });
  return out;
};

/** The text of a range's first line after its opening brace, to name it in assertions. */
const firstLineOf = (v: EditorView, r: { from: number; to: number } | null) => (r ? v.state.doc.lineAt(r.from).text.trim() : null);

describe("enclosingFoldRange", () => {
  it("finds the method body from the method's statements", () => {
    const v = editor("return $this->hasMany");
    expect(firstLineOf(v, enclosingFoldRange(v.state, v.state.selection.main.head))).toBe("{");
    const r = enclosingFoldRange(v.state, v.state.selection.main.head)!;
    expect(v.state.sliceDoc(r.from, r.to)).toContain("hasMany");
    expect(v.state.sliceDoc(r.from, r.to)).not.toContain("totalSpent");
  });

  it("finds the method body from the signature line when the brace is on the next line", () => {
    const v = editor("public function orders", 4);
    const r = enclosingFoldRange(v.state, v.state.selection.main.head)!;
    expect(v.state.sliceDoc(r.from, r.to)).toContain("hasMany");
    expect(v.state.sliceDoc(r.from, r.to)).not.toContain("totalSpent");
  });

  it("finds the innermost block inside nested blocks", () => {
    const v = editor("$sum += $order");
    const r = enclosingFoldRange(v.state, v.state.selection.main.head)!;
    expect(v.state.sliceDoc(r.from, r.to).trim()).toBe("$sum += $order->total;");
  });

  it("finds the class body between its methods", () => {
    const v = editor("\n\n    public function totalSpent", 1);
    const r = enclosingFoldRange(v.state, v.state.selection.main.head)!;
    expect(v.state.sliceDoc(r.from, r.to)).toContain("orders");
    expect(v.state.sliceDoc(r.from, r.to)).toContain("totalSpent");
  });

  it("finds nothing outside every region", () => {
    const v = editor("<?php");
    expect(enclosingFoldRange(v.state, 0)).toBeNull();
  });
});

describe("Collapse / Expand", () => {
  it("collapses the method from inside its body (#218) and keeps the caret visible", () => {
    const v = editor("return $this->hasMany");
    expect(collapseRegion(v)).toBe(true);
    expect(folds(v)).toEqual(["return $this->hasMany(Order::class);"]);
    const [fold] = (() => {
      const r: number[] = [];
      foldedRanges(v.state).between(0, v.state.doc.length, (from) => void r.push(from));
      return r;
    })();
    expect(v.state.selection.main.head).toBe(fold);
  });

  it("collapses the method from its signature line (#218)", () => {
    const v = editor("public function orders", 10);
    expect(collapseRegion(v)).toBe(true);
    expect(folds(v)).toEqual(["return $this->hasMany(Order::class);"]);
  });

  it("collapses the parent region when pressed again", () => {
    const v = editor("$sum += $order");
    collapseRegion(v);
    collapseRegion(v);
    const outer: string[] = [];
    foldedRanges(v.state).between(0, v.state.doc.length, (from, to) => void outer.push(v.state.sliceDoc(from, to)));
    expect(outer.some((t) => t.includes("$sum = 0;") && t.includes("return $sum;"))).toBe(true);
  });

  it("expands the fold at the caret", () => {
    const v = editor("return $this->hasMany");
    collapseRegion(v);
    expect(expandRegion(v)).toBe(true);
    expect(folds(v)).toEqual([]);
    expect(expandRegion(v)).toBe(false);
  });

  it("collapses recursively: the region and every region inside it", () => {
    const v = editor("$sum = 0;");
    expect(collapseRegionRecursively(v)).toBe(true);
    const ranges: string[] = [];
    foldedRanges(v.state).between(0, v.state.doc.length, (from, to) => void ranges.push(v.state.sliceDoc(from, to).trim()));
    expect(ranges).toHaveLength(2);
    expect(ranges.some((t) => t === "$sum += $order->total;")).toBe(true);
  });

  it("expands recursively: the region and every fold inside it", () => {
    const v = editor("$sum = 0;");
    collapseRegionRecursively(v);
    expect(expandRegionRecursively(v)).toBe(true);
    expect(folds(v)).toEqual([]);
  });

  it("does nothing where there is nothing to fold", () => {
    const v = editor("<?php");
    expect(collapseRegion(v)).toBe(false);
    expect(collapseRegionRecursively(v)).toBe(false);
    expect(expandRegionRecursively(v)).toBe(false);
  });
});
