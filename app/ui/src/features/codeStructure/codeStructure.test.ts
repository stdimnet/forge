import { describe, expect, it } from "vitest";
import { EditorState, Text } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { foldable } from "@codemirror/language";
import { docCommentsOf, groupByRegions, scopesAt, structureLanguages, structureLoaded, structureOf, type Scope } from "./model";
import { stickyScopes } from "./sticky";
import { crumbsAt } from "./breadcrumbs";
import { regionFolding, regionRange } from "./regions";
import { allRendered, docCommentAtLine, renderedDocs, renderedDocsField, setRenderAll, toggleDocAtLine } from "./renderedDocs";

await structureLoaded;

// jsdom has no layout; CodeMirror measures text ranges after updates.
const noRects = () => Object.assign([], { item: () => null }) as unknown as DOMRectList;
Range.prototype.getClientRects ??= noRects;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

const PHP = `<?php

/**
 * A cart.
 *
 * @param int $id The id
 */
class Cart
{
    // region Totals
    public function total(array $lines): float
    {
        foreach ($lines as $line) {
            if ($line) {
                $sum = 1;
            }
        }
        return 0.0;
    }
    // endregion
}
`;

const doc = Text.of(PHP.split("\n"));

describe("editor structure", () => {
  it("comes from Rust and is cached per document version", () => {
    const s = structureOf(doc, "php");
    expect(s.scopes.map((x) => x.name)).toEqual(["Cart", "total()", "foreach", "if"]);
    expect(structureOf(doc, "php")).toBe(s);
    expect(s.regions).toMatchObject([{ name: "Totals", line: 9, endLine: 19 }]);
    expect(structureOf(doc, undefined).scopes).toEqual([]);
    expect(structureLanguages().map((l) => l.id)).toContain("vue");
  });

  it("finds the scopes around a position", () => {
    const s = structureOf(doc, "php");
    expect(scopesAt(s, 15, 4).map((x) => x.name)).toEqual(["Cart", "total()", "foreach", "if"]);
    expect(scopesAt(s, 2, 0)).toEqual([]);
  });

  it("picks the sticky lines for a scroll position", () => {
    const s = structureOf(doc, "php").scopes;
    // The top line is 12 (`foreach`): the class, the method and the loop stick.
    const lineAt = (row: number) => 12 + row;
    expect(stickyScopes(s, lineAt, 5).map((x: Scope) => x.name)).toEqual(["Cart", "total()", "foreach"]);
    expect(stickyScopes(s, lineAt, 2).map((x) => x.name)).toEqual(["Cart", "total()"]);
    // Near the end of the method only the class is left.
    expect(stickyScopes(s, (row) => 17 + row, 5).map((x) => x.name)).toEqual(["Cart"]);
  });

  it("puts the caret's declarations into the breadcrumbs", () => {
    const pos = PHP.indexOf("$sum");
    const view = new EditorView({ state: EditorState.create({ doc: PHP, selection: { anchor: pos } }) });
    expect(crumbsAt(view, "php").map((c) => c.scope.name)).toEqual(["Cart", "total()"]);
    view.destroy();
  });

  it("groups Structure view nodes under regions", () => {
    const nodes = [{ name: "Cart", line: 7, children: [{ name: "total", line: 10 }] }];
    const grouped = groupByRegions(nodes, structureOf(doc, "php").regions);
    expect(grouped[0].children?.[0]).toMatchObject({ name: "Totals", kind: "region" });
  });
});

describe("custom folding regions", () => {
  it("fold from the region comment to the end comment", () => {
    const state = EditorState.create({ doc: PHP, extensions: regionFolding("php") });
    const region = structureOf(state.doc, "php").regions[0];
    const range = regionRange(state, region)!;
    expect(state.sliceDoc(range.from, range.from + 16)).toBe("// region Totals");
    expect(state.sliceDoc(range.to - 12, range.to)).toBe("// endregion");
    const line = state.doc.line(10);
    expect(foldable(state, line.from, line.to)).toEqual(range);
  });
});

describe("rendered doc comments", () => {
  it("are parsed in Rust and toggled per comment and per file", () => {
    const comments = docCommentsOf(doc, "php");
    expect(comments).toHaveLength(1);
    expect(comments[0].doc.sections[0].title).toBe("Params:");

    const view = new EditorView({ state: EditorState.create({ doc: PHP, extensions: renderedDocs("php", () => false) }) });
    expect(view.state.field(renderedDocsField).deco.size).toBe(0);
    // On the declaration below the comment.
    expect(docCommentAtLine(view.state, 7)?.line).toBe(2);
    expect(toggleDocAtLine(view, 7)).toBe(true);
    expect(view.state.field(renderedDocsField).deco.size).toBe(1);
    expect(toggleDocAtLine(view, 15)).toBe(false);

    view.dispatch({ effects: setRenderAll.of(true) });
    expect(allRendered(view.state)).toBe(true);
    expect(view.state.field(renderedDocsField).deco.size).toBe(1);
    view.destroy();
  });
});
