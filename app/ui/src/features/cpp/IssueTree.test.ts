import { describe, expect, it, vi } from "vitest";

const openFileAt = vi.hoisted(() => vi.fn(() => Promise.resolve(true)));
vi.mock("@/editor/views", async (orig) => ({ ...(await orig<typeof import("@/editor/views")>()), openFileAt }));

const { openFrame } = await import("./IssueTree");

describe("native issue frames", () => {
  it("open at the reported 1-based column", () => {
    openFrame({ index: 0, function: "main", line: 12, column: 5, path: "src/main.cpp" });
    expect(openFileAt).toHaveBeenLastCalledWith("src/main.cpp", 12, 5, { flash: true });
    openFrame({ index: 0, function: "main", line: 0, column: 0, path: "src/main.cpp" });
    expect(openFileAt).toHaveBeenLastCalledWith("src/main.cpp", 1, undefined, { flash: true });
  });
});
