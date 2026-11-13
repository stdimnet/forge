import { describe, expect, it } from "vitest";
import { ssrAnswer, ssrRequest, type SsrForm } from "./state";

const form: SsrForm = {
  template: "$o$->save()",
  replacement: "$o$->persist()",
  fileType: "php",
  matchCase: true,
  scopeName: "Project Files",
  constraints: { o: { name: "o", text: "order" } },
};

describe("Structural Search and Replace", () => {
  it("builds the request from the dialog", () => {
    const search = ssrRequest(form, false);
    expect(search.replacement).toBeUndefined();
    expect(search.scope).toBeUndefined();
    expect(search.constraints).toEqual([{ name: "o", text: "order" }]);
    expect(ssrRequest(form, true).replacement).toBe("$o$->persist()");
  });

  it("shows matches as one group of files with their replacement", () => {
    const answer = ssrAnswer("$o$->save()", {
      files: [
        {
          path: "app/A.php",
          matches: [
            { line: 2, column: 4, endLine: 2, endColumn: 17, start: 30, end: 43, text: "$order->save();", ranges: [[0, 13]], matched: "$order->save()", bindings: [], replacement: "$order->persist()" },
          ],
        },
      ],
      matchCount: 1,
      searchedFiles: 3,
      truncated: false,
    });
    expect(answer.byFile.total).toBe(1);
    expect(answer.byFile.groups[0].files[0].usages[0]).toMatchObject({ line: 2, column: 4, after: "$order->persist()" });
  });
});
