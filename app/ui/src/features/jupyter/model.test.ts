import { describe, expect, it } from "vitest";
import type { CellDiff } from "@/ipc/jupyter";
import { focusContextOf } from "@/keymap/dispatch";
import { countLabel, kernelLabel, moveTarget, newCell, newCellId, outputText, selectionAfterRemove, toCellState, toNotebook } from "./model";
import { groupCells } from "./NotebookDiffView";
import { frameColumns } from "./DataFrameView";

describe("notebook model", () => {
  it("makes unique Jupyter-like cell ids", () => {
    let n = 0;
    const seq = [0.5, 0.5, 0.25];
    const id = newCellId(["7fffffff"], () => seq[n++]);
    expect(id).toBe("3fffffff");
    expect(newCellId([])).toMatch(/^[0-9a-f]{8}$/);
  });

  it("saves the model without the run state", () => {
    const md = { ...newCell("markdown", []), source: "# T", running: true };
    const code = toCellState(
      { id: "c", cellType: "code", source: "1", metadata: { tags: [] }, executionCount: 3, outputs: [{ outputType: "stream", name: "stdout", text: "x" }], origin: 1 },
      [],
    );
    const nb = toNotebook({ nbformat: 4, nbformatMinor: 5, metadata: {}, language: "python", hasIds: true }, [md, code]);
    expect(nb.cells[0]).toEqual({ id: md.id, cellType: "markdown", source: "# T", metadata: {}, executionCount: null, outputs: [] });
    expect(nb.cells[1]).toMatchObject({ id: "c", executionCount: 3, origin: 1, metadata: { tags: [] } });
    expect(nb.cells[1]).not.toHaveProperty("running");
  });

  it("moves, removes and labels", () => {
    expect(moveTarget(0, -1, 3)).toBeNull();
    expect(moveTarget(1, 1, 3)).toBe(2);
    expect(moveTarget(2, 1, 3)).toBeNull();
    expect(selectionAfterRemove(2, 3)).toBe(1);
    expect(selectionAfterRemove(0, 1)).toBe(0);
    expect(countLabel({ running: true, executionCount: 2 })).toBe("[*]");
    expect(countLabel({ running: false, executionCount: null })).toBe("[ ]");
    expect(countLabel({ running: false, executionCount: 7 })).toBe("[7]");
    expect(kernelLabel("idle")).toBe("Idle");
    expect(kernelLabel("none")).toBe("Not started");
    expect(outputText({ outputType: "error", ename: "E", evalue: "v", traceback: ["\x1b[0;31mE\x1b[0m: v"] })).toBe("E: v");
  });

  it("folds runs of unchanged cells in a diff", () => {
    const cell = (status: CellDiff["status"]): CellDiff => ({
      status,
      source: { changed: false, inserted: 0, deleted: 0, lines: [] },
      outputs: { changed: false, inserted: 0, deleted: 0, lines: [] },
      typeChanged: false,
      metadataChanged: false,
      executionCountChanged: false,
    });
    const groups = groupCells([cell("unchanged"), cell("unchanged"), cell("modified"), cell("unchanged"), cell("added")]);
    expect(groups.map((g) => (Array.isArray(g) ? g.length : g.status))).toEqual([2, "modified", 1, "added"]);
  });

  it("names DataFrame columns for the grid", () => {
    expect(frameColumns({ columns: ["", "a", "a", ""], indexColumns: 1, rows: [] })).toEqual(["index", "a", "a (2)", "column 4"]);
  });

  it("tells command mode from a cell editor", () => {
    document.body.innerHTML = `
      <div class="nb-cells" tabindex="0" data-focus-context="notebook">
        <div class="cm-editor"><div class="cm-content" contenteditable="true"></div></div>
        <div data-focus-context="notebook-output"><div class="dg" tabindex="0"></div><input id="filter" type="text"></div>
      </div>`;
    const q = (s: string) => document.querySelector(s);
    expect(focusContextOf(q(".nb-cells"))).toBe("notebook");
    expect(focusContextOf(q(".cm-content"))).toBe("notebook-editor");
    expect(focusContextOf(q(".dg"))).toBe("global");
    expect(focusContextOf(q("#filter"))).toBe("input");
  });
});
