import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./gitMock";
import "./gitChangesMock";
import { invoke } from "@/ipc";
import { gitApi } from "@/features/git/api";
import { changesApi } from "@/features/git/changesApi";

const USER = "app/Models/User.php";
const opts = { amend: false, signOff: false, noVerify: false, author: null };

describe("browser-mode changelists, partial commits and patches", () => {
  it("lists, creates and moves changelists", async () => {
    const lists = await changesApi.changelists();
    expect(lists.map((l) => l.name)).toEqual(["Changes", "Checkout refactoring"]);
    expect(lists[0].active).toBe(true);
    expect(lists[1].files.map((f) => f.path)).toContain("app/Http/Controllers/OrderController.php");
    expect(lists[0].files.map((f) => f.path)).toContain(USER);

    const created = await changesApi.createChangelist("Docs", "Update docs", false);
    expect(created.map((l) => l.name)).toContain("Docs");
    await expect(changesApi.createChangelist("docs", "", false)).rejects.toThrow(/already exists/);

    // One hunk of User.php goes to another list: the file shows in both.
    const hunks = await changesApi.fileHunks(USER);
    expect(hunks.length).toBeGreaterThan(1);
    const docs = created.find((l) => l.name === "Docs")!;
    const moved = await changesApi.move([USER], docs.id, [hunks[0].id]);
    expect(moved.find((l) => l.id === docs.id)!.files).toEqual([{ path: USER, hunks: [hunks[0].id] }]);
    expect(moved[0].files.find((f) => f.path === USER)!.hunks).toEqual(hunks.slice(1).map((h) => h.id));

    // A new list by name, then set active and delete the other.
    const named = await changesApi.move([USER], "Users", null, true);
    const users = named.find((l) => l.name === "Users")!;
    expect(users.files).toEqual([{ path: USER, hunks: null }]);
    await changesApi.setActive(users.id);
    await expect(changesApi.deleteChangelist(users.id)).rejects.toThrow(/active/);
    const after = await changesApi.deleteChangelist(docs.id);
    expect(after.some((l) => l.id === docs.id)).toBe(false);
    await changesApi.setActive("default");
  });

  it("commits single hunks", async () => {
    const before = await invoke<string>("fs_read_file", { path: USER });
    const hunks = await changesApi.fileHunks(USER);
    const sha = await changesApi.commitFiles("Partial", [{ path: USER, hunks: [hunks[0].id] }], opts);
    expect((await gitApi.log({ limit: 1 }))[0].sha).toBe(sha);
    // The working copy is unchanged; one hunk fewer is left.
    expect(await invoke<string>("fs_read_file", { path: USER })).toBe(before);
    expect((await changesApi.fileHunks(USER)).length).toBe(hunks.length - 1);
    await expect(changesApi.commitFiles("x", [{ path: USER, hunks: null }], { ...opts, author: "nobody" })).rejects.toThrow(/author/);
  });

  it("creates, previews and applies patches", async () => {
    const path = "routes/web.php";
    const patch = await changesApi.createPatch([path]);
    expect(patch).toContain(`diff --git a/${path} b/${path}`);
    await changesApi.writePatch("changes.patch", patch);
    expect(await changesApi.readPatch("changes.patch")).toBe(patch);

    const current = await invoke<string>("fs_read_file", { path });
    await invoke("fs_write_file", { path, contents: (await gitApi.show("HEAD", path)) ?? "" });
    const preview = await changesApi.patchPreview(patch, "");
    expect(preview).toHaveLength(1);
    expect(preview[0].applies).toBe(true);
    expect(preview[0].after).toBe(current);
    const res = await changesApi.applyPatch(patch, "", null);
    expect(res.applied).toEqual([path]);
    expect(await invoke<string>("fs_read_file", { path })).toBe(current);

    // Applied again it no longer fits.
    const again = await changesApi.applyPatch(patch, "", null);
    expect(again.failed.map(([p]) => p)).toEqual([path]);
  });
});
