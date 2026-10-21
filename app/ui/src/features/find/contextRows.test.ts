import { describe, expect, it } from "vitest";
import { fileRows } from "./contextRows";

const hit = (line: number, before: string[], after: string[]) => ({ line, column: 0, length: 1, text: `m${line}`, ranges: [[0, 1]] as [number, number][], before, after });

describe("fileRows", () => {
  it("shows a context line once even when two matches share it", () => {
    const file = { path: "a.php", matches: [hit(2, ["l0", "l1"], ["l3", "l4"]), hit(5, ["l3", "l4"], ["l6"])] };
    const rows = fileRows(file).map((r) => (r.kind === "match" ? `match ${r.match.line}` : `ctx ${r.line} ${r.text}`));
    expect(rows).toEqual(["ctx 0 l0", "ctx 1 l1", "match 2", "ctx 3 l3", "ctx 4 l4", "match 5", "ctx 6 l6"]);
  });

  it("keeps plain matches as they are", () => {
    const file = { path: "a.php", matches: [{ line: 4, column: 0, length: 1, text: "x", ranges: [[0, 1]] as [number, number][] }] };
    expect(fileRows(file)).toHaveLength(1);
  });
});
