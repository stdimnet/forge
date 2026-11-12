import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./gitMock";
import "./gitOpsMock";
import { invoke } from "@/ipc";
import { gitApi } from "@/features/git/api";
import { opsApi } from "@/features/git/opsApi";
import { operationLabel } from "@/features/git/operation";
import { mockRepo } from "./gitMock";

describe("browser-mode git dialogs and shelf", () => {
  it("shelves, lists and unshelves changes", async () => {
    const before = await invoke<string>("fs_read_file", { path: "app/Models/User.php" });
    const list = await opsApi.shelfList();
    const cl = await opsApi.shelve(["app/Models/User.php"], "Shelved user");
    expect(cl.files.map((f) => f.path)).toEqual(["app/Models/User.php"]);
    expect((await opsApi.shelfList()).length).toBe(list.length + 1);
    const reverted = await invoke<string>("fs_read_file", { path: "app/Models/User.php" });
    expect(reverted).not.toBe(before);
    const diff = await opsApi.shelfFile(cl.id, "app/Models/User.php");
    expect(diff.after).toBe(before);
    await opsApi.shelfRename(cl.id, "Renamed");
    expect((await opsApi.shelfList()).find((c) => c.id === cl.id)?.name).toBe("Renamed");
    await opsApi.unshelve(cl.id, null, true);
    expect(await invoke<string>("fs_read_file", { path: "app/Models/User.php" })).toBe(before);
    expect((await opsApi.shelfList()).some((c) => c.id === cl.id)).toBe(false);
  });

  it("shows stash files and clears stashes", async () => {
    const stashes = await gitApi.stashList();
    expect(stashes.length).toBeGreaterThan(0);
    const files = await opsApi.stashFiles(0);
    expect(files.length).toBeGreaterThan(0);
    const c = await opsApi.stashFile(0, files[0].path);
    expect(c.after).not.toBe(c.before);
    await opsApi.stashClear();
    expect(await gitApi.stashList()).toEqual([]);
  });

  it("stops a rebase onto the conflict branch and aborts it", async () => {
    await expect(opsApi.rebase({ onto: "feature/checkout-flow" })).rejects.toThrow(/CONFLICT/);
    const state = await opsApi.operationState();
    expect(state.operation).toBe("rebase");
    expect(operationLabel(state)).toMatch(/^Rebasing /);
    await expect(opsApi.operationAction("continue")).rejects.toThrow(/merge conflicts/);
    await opsApi.operationAction("abort");
    expect((await opsApi.operationState()).operation).toBeNull();
  });

  it("updates a branch that is not checked out", async () => {
    const { state, bySha } = mockRepo;
    const before = await gitApi.branches();
    const main = before.local.find((b) => b.name === "main")!;
    expect(main.current).toBe(false);
    expect((await opsApi.updateBranch("main")).output).toBe("Already up to date.");
    // The remote moves on: main is fast-forwarded, the current branch and HEAD stay.
    const ahead = [...bySha.values()].find((c) => c.parents.includes(main.sha))!.sha;
    state.remote.set("origin/main", ahead);
    const { output } = await opsApi.updateBranch("main");
    expect(output).toBe(`${main.sha.slice(0, 8)}..${ahead.slice(0, 8)}`);
    const after = await gitApi.branches();
    expect(after.local.find((b) => b.name === "main")!.sha).toBe(ahead);
    expect(after.current).toBe(before.current);
    expect(state.head).toBe(before.local.find((b) => b.current)!.sha);
    // A branch with commits of its own is never rewritten.
    state.remote.set("origin/main", main.sha);
    await expect(opsApi.updateBranch("main")).rejects.toThrow(/commits that are not in its tracked branch/);
    await expect(opsApi.updateBranch(before.current!)).rejects.toThrow(/is checked out/);
    await expect(opsApi.updateBranch("bugfix/login-redirect")).rejects.toThrow(/no tracked branch/);
    state.local.set("main", main.sha);
  });

  it("edits remotes and creates forced tags", async () => {
    await opsApi.editRemote("origin", "upstream", "git@example.com:acme/shop.git");
    expect((await gitApi.remotes()).map((r) => r.name)).toContain("upstream");
    await opsApi.tag({ name: "v9.9.9" });
    await expect(opsApi.tag({ name: "v9.9.9" })).rejects.toThrow(/already exists/);
    await opsApi.tag({ name: "v9.9.9", force: true });
    expect(await opsApi.resolveCommit("v9.9.9")).toBeTruthy();
  });
});
