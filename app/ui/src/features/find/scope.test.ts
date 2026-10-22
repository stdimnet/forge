import { describe, expect, it } from "vitest";
import { scopeRequest } from "./scope";

const sources = {
  openFiles: () => ["a.php", "b.php"],
  changedFiles: () => ["c.php", "c.php"],
  filter: (name: string) => (name === "Models" ? { scope: { pattern: "file:app/Models//*" } } : undefined),
};

describe("scopeRequest", () => {
  it("searches the project, a directory, the open files, the changed files or a scope", () => {
    expect(scopeRequest({ scope: "project", directory: "app" }, sources)).toEqual({ directory: "" });
    expect(scopeRequest({ scope: "directory", directory: "app" }, sources)).toEqual({ directory: "app" });
    expect(scopeRequest({ scope: "openFiles", directory: "app" }, sources)).toEqual({ directory: "", paths: ["a.php", "b.php"] });
    expect(scopeRequest({ scope: "changedFiles", directory: "" }, sources)).toEqual({ directory: "", paths: ["c.php"] });
    expect(scopeRequest({ scope: "scope", directory: "", scopeName: "Models" }, sources)).toEqual({
      directory: "",
      scope: { scope: { pattern: "file:app/Models//*" } },
    });
  });
});
