import { describe, expect, it } from "vitest";
import type { TodoResult } from "@/ipc/workbench";
import { filterTodos, mergeTodos, parsePatterns, patternLabel } from "./store";

const item = (line: number, pattern = 0) => ({ line, column: 3, length: 4, text: pattern ? "FIXME" : "TODO", pattern });

const base: TodoResult = {
  files: [
    { path: "app/A.php", items: [item(1), item(4, 1)] },
    { path: "app/B.php", items: [item(2)] },
  ],
  count: 3,
  truncated: false,
};

describe("TODO store", () => {
  it("parses pattern settings with the case flag", () => {
    expect(parsePatterns(["\\btodo\\b.*", "\\bHACK\\b.*\tcase", "  "])).toEqual([
      { pattern: "\\btodo\\b.*", caseSensitive: false },
      { pattern: "\\bHACK\\b.*", caseSensitive: true },
    ]);
    expect(patternLabel("\\bfixme\\b.*")).toBe("FIXME");
  });

  it("merges rescanned files into the last answer", () => {
    const update: TodoResult = { files: [{ path: "app/0.php", items: [item(9)] }], count: 1, truncated: false };
    const merged = mergeTodos(base, update, ["app/A.php", "app/0.php"]);
    expect(merged.files.map((f) => f.path)).toEqual(["app/0.php", "app/B.php"]);
    expect(merged.count).toBe(2);
  });

  it("filters by pattern and by file", () => {
    expect(filterTodos(base, new Set([0])).files).toEqual([{ path: "app/A.php", items: [item(4, 1)] }]);
    expect(filterTodos(base, new Set(), "app/B.php").count).toBe(1);
  });
});
