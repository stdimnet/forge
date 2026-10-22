import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./gitMock";
import "./gitHistoryMock";
import "./compareMock";
import { gitApi } from "@/features/git/api";
import { historyApi } from "@/features/git/historyApi";
import { compareApi } from "@/features/compare/api";
import { visibleRows } from "@/features/compare/DirDiffView";
import { invoke } from "@/ipc";

const USER = "app/Models/User.php";

describe("browser-mode file history", () => {
  it("lists the commits of a file and of a folder", async () => {
    const file = await historyApi.fileHistory(USER, false);
    expect(file.length).toBeGreaterThanOrEqual(5);
    expect(file.every((r) => r.files.length === 1 && r.files[0].path === USER)).toBe(true);
    expect(file.at(-1)!.subject).toBe("Add User model");
    const folder = await historyApi.fileHistory("app/Models", true);
    expect(folder.length).toBeGreaterThan(file.length);
    expect(folder.flatMap((r) => r.files).every((f) => f.path.startsWith("app/Models/"))).toBe(true);
    const page = await historyApi.fileHistory(USER, false, { skip: 1, limit: 2 });
    expect(page.map((r) => r.sha)).toEqual(file.slice(1, 3).map((r) => r.sha));
  });

  it("serves older revisions consistent with the blame", async () => {
    const [newest, , , , oldest] = (await historyApi.fileHistory(USER, false)).slice(0, 5);
    const first = await gitApi.show(oldest.sha, USER);
    const latest = await gitApi.show(newest.sha, USER);
    expect(first).not.toBeNull();
    expect(first!.length).toBeLessThan(latest!.length);
    const blame = await historyApi.blameRevision(oldest.sha, USER);
    expect(blame.lines.length).toBe(first!.split("\n").length - 1);
    expect(blame.commits.every((c) => c.time <= oldest.time)).toBe(true);
  });

  it("shows how a block of lines evolved", async () => {
    const revs = await historyApi.blockHistory(USER, 28, 40);
    expect(revs.length).toBeGreaterThanOrEqual(2);
    expect(revs[0].after).not.toBe(revs[0].before);
    expect(revs.at(-1)!.before).toBeNull();
    await expect(historyApi.blockHistory(USER, 5, 2)).rejects.toThrow();
  });

  it("lists the files of a folder that differ from a branch", async () => {
    const changes = await historyApi.changesSince("main", "app");
    const paths = changes.map((c) => c.path);
    expect(paths).toContain(USER);
    expect(paths.every((p) => p.startsWith("app/"))).toBe(true);
  });
});

describe("browser-mode Compare Directories", () => {
  it("compares two folders with the Rust folder diff and copies entries", async () => {
    const write = (path: string, contents: string) => invoke("fs_write_file", { path, contents });
    await write("cmp/left/same.txt", "x\n");
    await write("cmp/right/same.txt", "x\n");
    await write("cmp/left/changed.txt", "a\n");
    await write("cmp/right/changed.txt", "b\n");
    await write("cmp/left/sub/only-left.txt", "l\n");
    await write("cmp/right/only-right.txt", "r\n");

    const diff = await compareApi.dirs("cmp/left", "cmp/right");
    const rows = Object.fromEntries(diff.entries.map((e) => [e.path, e.status]));
    expect(rows).toEqual({
      sub: "leftOnly",
      "sub/only-left.txt": "leftOnly",
      "changed.txt": "different",
      "only-right.txt": "rightOnly",
      "same.txt": "equal",
    });

    const all = { equal: true, different: true, leftOnly: true, rightOnly: true };
    expect(visibleRows(diff.entries, { ...all, equal: false }, new Set()).map((e) => e.path)).not.toContain("same.txt");
    expect(visibleRows(diff.entries, all, new Set(["sub"])).map((e) => e.path)).not.toContain("sub/only-left.txt");

    await compareApi.copy("cmp/left/changed.txt", "cmp/right/changed.txt", false);
    await compareApi.copy("cmp/left/sub", "cmp/right/sub", true);
    const after = await compareApi.dirs("cmp/left", "cmp/right");
    expect(after.entries.filter((e) => e.status !== "equal").map((e) => e.path)).toEqual(["only-right.txt"]);
  });
});
