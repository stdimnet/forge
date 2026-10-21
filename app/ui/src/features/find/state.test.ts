import { describe, expect, it } from "vitest";
import type { FindResult } from "@/ipc/find";
import { neighbourUsage, usageItems } from "@/features/codeintel/usages";
import { findRequest, occurrencesAnswer } from "./state";

const result: FindResult = {
  files: [
    { path: "app/A.php", matches: [{ line: 3, column: 8, length: 5, text: "$total = $total + 1;", ranges: [[0, 6], [9, 15]] }] },
    { path: "app/B.php", matches: [{ line: 0, column: 2, length: 5, text: "total", ranges: [[0, 5]] }] },
  ],
  matchCount: 3,
  truncated: false,
};

describe("Find in Files in the Find tool window", () => {
  it("builds the request from the popup options", () => {
    const options = { caseSensitive: true, wholeWords: false, regex: false, maskEnabled: false, fileMask: "*.php", scope: "directory" as const, directory: "app" };
    expect(findRequest("total", options)).toEqual({
      query: "total",
      caseSensitive: true,
      wholeWords: false,
      regex: false,
      fileMask: "",
      directory: "app",
      limit: 1000,
    });
  });

  it("turns occurrences into a tree grouped by file", () => {
    const answer = occurrencesAnswer("total", result);
    expect(answer.byFile.total).toBe(3);
    expect(answer.byFile.fileCount).toBe(2);
    const rows = usageItems(answer.byFile);
    expect(rows.map((r) => `${r.path}:${r.row.line}`)).toEqual(["app/A.php:3", "app/B.php:0"]);
    expect(rows[0].row.ranges).toHaveLength(2);
    expect(rows[0].row.endColumn).toBe(13);
  });

  it("walks the occurrences with Next/Previous Occurrence", () => {
    const items = usageItems(occurrencesAnswer("total", result).byFile);
    expect(neighbourUsage(items, undefined, 1)?.path).toBe("app/A.php");
    expect(neighbourUsage(items, undefined, -1)?.path).toBe("app/B.php");
    expect(neighbourUsage(items, "app/A.php:3:8", 1)?.path).toBe("app/B.php");
    expect(neighbourUsage(items, "app/B.php:0:2", 1)?.path).toBe("app/A.php");
    expect(neighbourUsage([], undefined, 1)).toBeUndefined();
  });
});
