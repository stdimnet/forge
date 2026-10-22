import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./gitMock";
import { gitApi } from "@/features/git/api";
import { invoke } from "@/ipc";

describe("browser-mode git backend", () => {
  it("reports a believable working tree", async () => {
    const st = await gitApi.status();
    expect(st.branch).toBe("feature/order-totals");
    expect(st.ahead).toBe(1);
    const byPath = Object.fromEntries(st.entries.map((e) => [e.path, e]));
    expect(byPath["app/Models/User.php"].worktree).toBe("modified");
    expect(byPath["app/Services/Checkout.php"].index).toBe("added");
    expect(byPath["tests/Feature/OrderTest.php"].worktree).toBe("untracked");
    expect(byPath["app/Http/Middleware/RedirectIfGuest.php"].worktree).toBe("deleted");
  });

  it("serves a log with merges, refs and paging", async () => {
    const log = await gitApi.log({ all: true });
    expect(log.length).toBeGreaterThanOrEqual(25);
    expect(log.some((c) => c.parents.length === 2)).toBe(true);
    expect(log[0].refs.some((r) => r.current)).toBe(true);
    const page = await gitApi.log({ all: true, skip: 5, limit: 3 });
    expect(page.map((c) => c.sha)).toEqual(log.slice(5, 8).map((c) => c.sha));
    const details = await gitApi.commitDetails(log[1].sha);
    expect(details.files.length).toBeGreaterThan(0);
  });

  it("blames User.php with committed and uncommitted lines", async () => {
    const blame = await gitApi.blame("app/Models/User.php");
    const content = await invoke<string>("fs_read_file", { path: "app/Models/User.php" });
    expect(blame.lines.length).toBe(content.split("\n").length - 1);
    const authors = new Set(blame.commits.map((c) => c.author));
    expect(authors.size).toBeGreaterThanOrEqual(3);
    expect(blame.commits.some((c) => c.uncommitted)).toBe(true);
  });

  it("diffs against HEAD and commits selected files", async () => {
    const diff = await gitApi.diffHead(["app/Models/User.php"]);
    expect(diff).toContain("+    public function totalSpent(): float");
    const sha = await gitApi.commit("Add totalSpent helper", false, ["app/Models/User.php"]);
    const st = await gitApi.status();
    expect(st.head).toBe(sha);
    expect(st.entries.some((e) => e.path === "app/Models/User.php")).toBe(false);
    const [head] = await gitApi.log({ limit: 1 });
    expect(head.subject).toBe("Add totalSpent helper");
  });

  it("creates, checks out and deletes branches", async () => {
    await gitApi.createBranch("feature/tmp", null, true);
    expect((await gitApi.branches()).current).toBe("feature/tmp");
    await gitApi.checkout("main");
    await gitApi.deleteBranch("feature/tmp");
    const b = await gitApi.branches();
    expect(b.local.some((x) => x.name === "feature/tmp")).toBe(false);
    expect(b.current).toBe("main");
  });

  it("adds unversioned files to .gitignore in the in-memory project", async () => {
    const patterns = await gitApi.addToGitignore(["tests/Unit/CheckoutTest.php"]);
    expect(patterns).toEqual(["/tests/Unit/CheckoutTest.php"]);
    const text = await invoke<string>("fs_read_file", { path: ".gitignore" });
    expect(text.trimEnd().split("\n").pop()).toBe("/tests/Unit/CheckoutTest.php");
    const st = await gitApi.status();
    expect(st.entries.some((e) => e.path === "tests/Unit/CheckoutTest.php")).toBe(false);
    await expect(gitApi.addToGitignore(["../outside"])).rejects.toThrow();
  });

  it("creates tags, stashes and remotes", async () => {
    await gitApi.createTag("v9.9.9");
    expect((await gitApi.branches()).tags.some((t) => t.name === "v9.9.9")).toBe(true);

    const before = (await gitApi.stashList()).length;
    await invoke("fs_write_file", { path: "app/Models/Order.php", contents: "<?php // stashed\n" });
    await gitApi.stash("Try stash");
    const list = await gitApi.stashList();
    expect(list).toHaveLength(before + 1);
    expect(list[0].message).toBe("Try stash");
    expect(await invoke<string>("fs_read_file", { path: "app/Models/Order.php" })).not.toContain("stashed");
    await gitApi.unstash(0, true);
    expect(await invoke<string>("fs_read_file", { path: "app/Models/Order.php" })).toContain("stashed");
    expect(await gitApi.stashList()).toHaveLength(before);

    await gitApi.addRemote("upstream", "https://example.com/acme/shop.git");
    expect((await gitApi.remotes()).map((r) => r.name)).toEqual(["origin", "upstream"]);
    await gitApi.removeRemote("upstream");
    expect((await gitApi.remotes()).map((r) => r.name)).toEqual(["origin"]);
  });

  it("answers ai_complete with a commit message", async () => {
    const msg = await invoke<string>("ai_complete", { request: { messages: [{ role: "user", content: "app/Models/User.php" }] } });
    expect(msg.split("\n")[0].length).toBeLessThanOrEqual(72);
  });
});

describe("browser-mode merge conflicts", () => {
  it("stops a merge of feature/checkout-flow with conflicts and resolves them", async () => {
    await expect(gitApi.merge("feature/checkout-flow")).rejects.toThrow(/CONFLICT/);
    const set = await gitApi.conflicts();
    expect(set.operation).toBe("merge");
    expect(set.description).toMatch(/^Merging branch feature\/checkout-flow into branch /);
    expect(set.files.map((f) => [f.path, f.yours, f.theirs])).toEqual([
      ["app/Services/PriceCalculator.php", "modified", "modified"],
      ["app/Reports/SalesReport.php", "modified", "deleted"],
      ["public/images/logo.png", "modified", "modified"],
    ]);
    const st = await gitApi.status();
    expect(st.entries.filter((e) => e.conflict).length).toBe(3);
    const marked = await invoke<string>("fs_read_file", { path: "app/Services/PriceCalculator.php" });
    expect(marked).toContain("<<<<<<< HEAD");

    const file = await gitApi.mergeFile("app/Services/PriceCalculator.php");
    expect(file.model?.chunks.map((c) => c.kind)).toEqual(["right", "left", "conflict", "right", "conflict", "conflict"]);
    expect(file.model?.simpleConflicts).toBe(1);
    expect((await gitApi.mergeFile("app/Reports/SalesReport.php")).theirs).toBeNull();

    await gitApi.resolveMerge(file.path, file.yours!, false);
    await gitApi.acceptConflictSide(["app/Reports/SalesReport.php"], "theirs");
    await expect(invoke("fs_read_file", { path: "app/Reports/SalesReport.php" })).rejects.toThrow();
    await expect(gitApi.commit("Merge", false, null)).rejects.toThrow(/unmerged/);
    await gitApi.acceptConflictSide(["public/images/logo.png"], "yours");
    expect((await gitApi.conflicts()).files).toEqual([]);
    const sha = await gitApi.commit("Merge branch 'feature/checkout-flow'", false, ["app/Services/PriceCalculator.php"]);
    expect((await gitApi.commitDetails(sha)).parents.length).toBe(2);
  });
});
