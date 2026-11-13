import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./gitMock";
import "./forgesMock";
import { forgesApi } from "@/ipc/forges";

describe("browser-mode code host", () => {
  it("detects the Forgejo repository with an account", async () => {
    const ctx = await forgesApi.repo();
    expect(ctx?.repo).toMatchObject({ kind: "forgejo", path: "acme/acme-shop", server: "https://git.acme.dev" });
    expect(ctx?.account?.username).toBe("marlon");
    expect(ctx?.capabilities.requestName).toBe("Pull Request");
  });

  it("lists and filters pull requests", async () => {
    const open = await forgesApi.mergeRequests({ state: "open" });
    expect(open.items.length).toBeGreaterThanOrEqual(6);
    expect(open.items.every((m) => m.state === "open")).toBe(true);
    const all = await forgesApi.mergeRequests({ state: "all" });
    expect(all.items.length).toBeGreaterThanOrEqual(8);
    const mine = await forgesApi.mergeRequests({ state: "all", author: "marlon" });
    expect(mine.items.every((m) => m.author.username === "marlon")).toBe(true);
    const search = await forgesApi.mergeRequests({ state: "all", search: "stripe" });
    expect(search.items.map((m) => m.number)).toEqual([36]);
  });

  it("serves inline threads anchored on changed User.php lines", async () => {
    const files = await forgesApi.files(41);
    const user = files.find((f) => f.path === "app/Models/User.php")!;
    const newLines = new Set(
      user.hunks.flatMap((h) => {
        let n = h.newStart;
        return h.lines.flatMap((l) => (l.kind === "removed" ? [] : [n++]));
      }),
    );
    const threads = (await forgesApi.discussions(41)).filter((d) => d.position?.path === "app/Models/User.php");
    expect(threads.length).toBeGreaterThanOrEqual(3);
    for (const t of threads) expect(newLines.has(t.position!.newLine!)).toBe(true);
  });

  it("comments, approves, merges and reports pipelines", async () => {
    await forgesApi.comment(34, "Thanks!");
    expect((await forgesApi.discussions(34)).some((d) => d.notes.some((n) => n.body === "Thanks!"))).toBe(true);
    await forgesApi.approve(34);
    expect((await forgesApi.mergeRequest(34)).approvals.approvedBy.map((u) => u.username)).toContain("marlon");
    await forgesApi.merge(34, { method: "squash", deleteSourceBranch: true });
    expect((await forgesApi.mergeRequest(34)).state).toBe("merged");
    await expect(forgesApi.merge(36, { method: "merge", deleteSourceBranch: false })).rejects.toThrow(/conflicts/);
    expect((await forgesApi.ciStatus("feature/order-totals"))?.status).toBe("running");
    expect((await forgesApi.issues({ state: "open" })).items.length).toBeGreaterThanOrEqual(5);
  });
});
