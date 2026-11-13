import { describe, expect, it } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { BuildProblem } from "@/ipc/cpp";

const { openTarget } = await import("./BuildProblems");

const problem = (p: Partial<BuildProblem>): BuildProblem => ({ file: "", line: 0, column: 0, severity: "error", message: "", tool: "gcc", ...p });

describe("C/C++ build problems", () => {
  it("open project files, and files outside the project as read-only library files", () => {
    expect(openTarget(problem({ path: "src/stats.cpp", line: 21 }))).toBe("src/stats.cpp");
    const header = "/usr/include/c++/13/numeric";
    expect(openTarget(problem({ path: header, library: `library://file/${header}`, line: 228 }))).toBe(`library://file/${header}`);
    // Outside the project and not a file on this machine, or no file at all: nothing to open.
    expect(openTarget(problem({ path: "/gone/a.h", line: 3 }))).toBeUndefined();
    expect(openTarget(problem({ path: "C:/gone/a.h" }))).toBeUndefined();
    expect(openTarget(problem({ path: null }))).toBeUndefined();
  });
});
