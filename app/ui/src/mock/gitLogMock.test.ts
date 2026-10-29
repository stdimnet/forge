import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./gitMock";
import "./gitOpsMock";
import "./gitLogMock";
import { gitApi } from "@/features/git/api";
import { logApi } from "@/features/git/logApi";

const subjects = async (n: number) => (await gitApi.log({ skip: 0, limit: n, all: false })).map((c) => c.subject);

describe("browser-mode Git Log actions", () => {
  it("knows which commits are pushed and protected", async () => {
    const [head, pushed] = await gitApi.log({ skip: 0, limit: 2, all: false });
    const top = await logApi.actions([head.sha]);
    expect(top.undoCommit.enabled).toBe(true);
    expect(top.cherryPick.enabled).toBe(false);
    const second = await logApi.actions([pushed.sha]);
    expect(second.undoCommit.enabled).toBe(false);
    expect(second.editMessage.warning).toContain("origin/");
    const old = (await gitApi.log({ skip: 0, limit: 30, all: false })).find((c) => c.subject === "Add Order model")!;
    const states = await logApi.actions([old.sha]);
    expect(states.interactiveRebase.enabled).toBe(false);
    expect(states.revert.enabled).toBe(true);
  });

  it("plans and replays an interactive rebase, then undoes it", async () => {
    const log = await gitApi.log({ skip: 0, limit: 3, all: false });
    const plan = await logApi.rebasePlan(log[2].sha);
    expect(plan.commits.map((c) => c.sha)).toEqual(log.map((c) => c.sha).reverse());
    const entries = plan.entries.map((e, i) => (i === 1 ? { ...e, action: "squash" as const, message: "United" } : e));
    const res = await logApi.interactiveRebase(log[2].sha, entries);
    expect(await subjects(2)).toEqual([log[0].subject, "United"]);
    await logApi.undoRewrite(res.oldHead, res.newHead!);
    expect(await subjects(3)).toEqual(log.map((c) => c.subject));
  });

  it("rewords, drops and squashes", async () => {
    const log = await gitApi.log({ skip: 0, limit: 3, all: false });
    await logApi.reword(log[1].sha, "Reworded\n\nBody");
    expect(await subjects(2)).toEqual([log[0].subject, "Reworded"]);
    const after = await gitApi.log({ skip: 0, limit: 3, all: false });
    await logApi.drop([after[0].sha]);
    expect(await subjects(1)).toEqual(["Reworded"]);
    const two = await gitApi.log({ skip: 0, limit: 2, all: false });
    await logApi.squash(two.map((c) => c.sha), "Squashed");
    expect(await subjects(1)).toEqual(["Squashed"]);
  });

  it("cherry-picks, reverts, undoes and lists revision files", async () => {
    const all = await gitApi.log({ skip: 0, limit: 50, all: true });
    const other = all.find((c) => c.subject === "Extract Checkout service from OrderController")!;
    await logApi.cherryPick([other.sha]);
    expect(await subjects(1)).toEqual([other.subject]);
    const head = (await gitApi.log({ skip: 0, limit: 1, all: false }))[0];
    await logApi.revert([head.sha]);
    expect(await subjects(1)).toEqual([`Revert "${other.subject}"`]);
    const top = (await gitApi.log({ skip: 0, limit: 1, all: false }))[0];
    expect((await logApi.undoCommit(top.sha)).message).toContain("Revert");
    expect(await logApi.treeFiles(top.sha)).toContain("composer.json");
    expect(await logApi.patch([other.sha])).toContain("diff --git");
  });
});
