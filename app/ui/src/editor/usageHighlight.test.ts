import { afterEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { stripeMarks } from "./errorStripe";
import { clearUsageHighlights, gotoHighlightedUsage, hasStickyUsages, setUsageHighlights, usageHighlighting, usageHighlights } from "./usageHighlight";

const views: EditorView[] = [];
afterEach(() => views.splice(0).forEach((v) => v.destroy()));

function make(doc: string): EditorView {
  const view = new EditorView({ state: EditorState.create({ doc, extensions: [usageHighlighting()] }) });
  views.push(view);
  return view;
}

const doc = "$a = 1;\necho $a;\n$a++;\n";
const marks = [
  { from: 0, to: 2, kind: "write" as const },
  { from: 13, to: 15, kind: "read" as const },
  { from: 17, to: 19, kind: "write" as const },
];

describe("usage highlighting", () => {
  it("drops the automatic highlight on edits and moves the sticky one along", () => {
    const view = make(doc);
    view.dispatch({ effects: setUsageHighlights.of({ marks, sticky: false }) });
    expect(view.state.facet(stripeMarks).map((m) => m.cls)).toEqual(["cm-stripe-usage-write", "cm-stripe-usage-read", "cm-stripe-usage-write"]);
    view.dispatch({ changes: { from: 0, insert: "\n" } });
    expect(usageHighlights(view.state)).toBeNull();
    expect(view.state.facet(stripeMarks)).toEqual([]);

    view.dispatch({ effects: setUsageHighlights.of({ marks: marks.map((m) => ({ ...m, from: m.from + 1, to: m.to + 1 })), sticky: true }) });
    view.dispatch({ changes: { from: 0, insert: "<?php" } });
    expect(hasStickyUsages(view.state)).toBe(true);
    expect(usageHighlights(view.state)!.marks[0]).toEqual({ from: 6, to: 8, kind: "write" });
    expect(clearUsageHighlights(view)).toBe(true);
    expect(clearUsageHighlights(view)).toBe(false);
  });

  it("F3 / Shift+F3 walk the sticky usages, wrapping around", () => {
    const view = make(doc);
    expect(gotoHighlightedUsage(view, 1)).toBe(false);
    view.dispatch({ effects: setUsageHighlights.of({ marks, sticky: true }) });
    const at = () => view.state.selection.main.from;
    expect(gotoHighlightedUsage(view, 1)).toBe(true);
    expect(at()).toBe(13);
    gotoHighlightedUsage(view, 1);
    expect(at()).toBe(17);
    gotoHighlightedUsage(view, 1);
    expect(at()).toBe(0);
    gotoHighlightedUsage(view, -1);
    expect(at()).toBe(17);
  });
});
