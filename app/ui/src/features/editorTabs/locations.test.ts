import { describe, expect, it } from "vitest";
import { movePath, recordLocation, shiftLocations, snippet } from "./locations";

describe("recent locations", () => {
  it("merges nearby places and keeps the newest first", () => {
    let l = recordLocation([], "a.php", 10, false);
    l = recordLocation(l, "b.php", 5, false);
    l = recordLocation(l, "a.php", 12, true);
    expect(l.map((x) => [x.path, x.line, x.changed])).toEqual([
      ["a.php", 12, true],
      ["b.php", 5, false],
    ]);
    // A visit next to a changed place keeps the change mark.
    l = recordLocation(l, "a.php", 13, false);
    expect(l[0].changed).toBe(true);
  });

  it("follows inserted lines, renames and deletions", () => {
    let l = recordLocation(recordLocation([], "a/x.php", 20, false), "a/x.php", 2, false);
    l = shiftLocations(l, "a/x.php", 5, 3);
    expect(l.map((x) => x.line)).toEqual([2, 23]);
    l = movePath(l, "a", "b");
    expect(l[0].path).toBe("b/x.php");
    expect(movePath(l, "b/x.php", null)).toEqual([]);
  });

  it("cuts a snippet around the line", () => {
    expect(snippet("1\n2\n3\n4", 1)).toEqual([
      [1, "1"],
      [2, "2"],
    ]);
    expect(snippet("1\n2\n3\n4", 3)).toEqual([
      [2, "2"],
      [3, "3"],
      [4, "4"],
    ]);
  });
});
