import { beforeAll, describe, expect, it } from "vitest";
import { forgeWasm } from "@/wasm";
import { checkPattern, editPattern, scopeContains, scopeEntries, scopeTree } from "./model";

beforeAll(async () => {
  await forgeWasm();
  // The model marks the module as loaded on the next tick.
  await new Promise((r) => setTimeout(r, 0));
});

describe("scopes through Rust", () => {
  it("lists predefined and named scopes", () => {
    const { entries, problems } = scopeEntries({ shared: [{ name: "Models", pattern: "file:app/Models//*" }], local: [{ name: "Bad", pattern: "file:a||" }] });
    expect(entries.map((e) => e.name)).toContain("Project Test Files");
    expect(entries.find((e) => e.name === "Models")?.storage).toBe("shared");
    expect(problems[0][0]).toBe("Bad");
  });

  it("checks and edits patterns", () => {
    expect(checkPattern("file:app//*")).toBeNull();
    expect(checkPattern("file:")?.message).toBe("File pattern expected");
    expect(editPattern("file:app//*", "excludeRecursively", "app/Legacy", true)).toBe("file:app//*&&!file:app/Legacy//*");
  });

  it("builds the preview tree and answers membership", () => {
    const tree = scopeTree(["app/A.php", "tests/ATest.php"], { scope: { predefined: "projectTestFiles" } });
    expect([tree.matched, tree.total]).toEqual([1, 2]);
    expect(scopeContains({ scope: { pattern: "file:*.php&&!test:*..*" } }, ["app/A.php", "tests/ATest.php"])).toEqual([true, false]);
  });
});
