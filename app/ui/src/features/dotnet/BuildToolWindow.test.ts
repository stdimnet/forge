import { describe, expect, it } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { BuildProblem } from "@/ipc/dotnet";

const { place } = await import("./BuildToolWindow");

const problem = (p: Partial<BuildProblem>): BuildProblem =>
  ({ severity: "error", code: "CS1001", message: "", line: 0, column: 0, endLine: 0, endColumn: 0, ...p }) as BuildProblem;

describe(".NET build problems", () => {
  it("show the first character of a file as :1:1", () => {
    expect(place(problem({ path: "src/A.cs", located: true }))).toBe("A.cs:1:1");
    expect(place(problem({ path: "src/A.cs", line: 4, column: 2, located: true }))).toBe("A.cs:5:3");
    // A project file problem has no position.
    expect(place(problem({ path: "src/A.csproj" }))).toBe("A.csproj");
  });
});
