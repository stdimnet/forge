import { describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import type { TodoResult } from "@/ipc/workbench";

const m = vi.hoisted(() => ({ replies: [] as ((r: unknown) => void)[] }));
vi.mock("@/ipc/workbench", async (orig) => {
  const real = await orig<typeof import("@/ipc/workbench")>();
  return { ...real, todoApi: { ...real.todoApi, scan: () => new Promise((r) => m.replies.push(r)) } };
});
vi.mock("@/state/workspace", async (orig) => ({ ...(await orig<typeof import("@/state/workspace")>()), workspace: () => ({ root: "/p" }) }));

const { rescanPaths, scanProject, todoResult } = await import("./store");
const result = (files: TodoResult["files"]): TodoResult => ({ files, count: files.reduce((n, f) => n + f.items.length, 0), truncated: false });
const todo = { line: 0, column: 0, text: "TODO x", pattern: 0 } as unknown as TodoResult["files"][number]["items"][number];

describe("TODO rescans", () => {
  it("keep the newer answer for a file when an older one arrives last", async () => {
    const full = scanProject();
    m.replies.shift()!(result([{ path: "a.php", items: [todo] }]));
    await full;
    // The TODO is deleted: the second rescan answers first, the first (still with it) last.
    const first = rescanPaths(["a.php"], { "a.php": "// TODO x" });
    const second = rescanPaths(["a.php"], { "a.php": "" });
    m.replies[1](result([]));
    await second;
    m.replies[0](result([{ path: "a.php", items: [todo] }]));
    await first;
    expect(todoResult()?.count).toBe(0);
  });
});
