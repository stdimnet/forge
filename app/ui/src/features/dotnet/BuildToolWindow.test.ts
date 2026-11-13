import { describe, expect, it } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { BuildProblem } from "@/ipc/dotnet";

const { place, openTarget } = await import("./BuildToolWindow");

const problem = (p: Partial<BuildProblem>): BuildProblem =>
  ({ severity: "error", code: "CS1001", message: "", line: 0, column: 0, endLine: 0, endColumn: 0, ...p }) as BuildProblem;

describe(".NET build problems", () => {
  it("show the first character of a file as :1:1", () => {
    expect(place(problem({ path: "src/A.cs", located: true }))).toBe("A.cs:1:1");
    expect(place(problem({ path: "src/A.cs", line: 4, column: 2, located: true }))).toBe("A.cs:5:3");
    // A project file problem has no position.
    expect(place(problem({ path: "src/A.csproj" }))).toBe("A.csproj");
  });

  it("open project files, and files outside the project as read-only library files", () => {
    expect(openTarget(problem({ path: "src/A.cs" }))).toBe("src/A.cs");
    const sdk = "/usr/share/dotnet/sdk/8.0.100/Microsoft.Common.targets";
    expect(openTarget(problem({ path: sdk, library: `library://file/${sdk}` }))).toBe(`library://file/${sdk}`);
    expect(openTarget(problem({ path: "C:/sdk/a.targets", library: "library://file/C:/sdk/a.targets" }))).toBe("library://file/C:/sdk/a.targets");
    // Outside the project and not a file on this machine, or no file at all: nothing to open.
    expect(openTarget(problem({ path: "/gone/A.cs" }))).toBeUndefined();
    expect(openTarget(problem({ origin: "MSBUILD" }))).toBeUndefined();
  });
});
