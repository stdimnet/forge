import { describe, expect, it } from "vitest";
import { agentTaskMockTiming } from "./agentTasksMock";
import { agentTasksApi, type AgentTask } from "@/ipc/agentTasks";
import { setSetting } from "./settingsMock";

async function waitFor<T>(f: () => T | undefined | null | false, ms = 30000): Promise<T> {
  const start = Date.now();
  for (;;) {
    const v = f();
    if (v) return v;
    if (Date.now() - start > ms) throw new Error("timed out");
    await new Promise((r) => setTimeout(r, 20));
  }
}

describe("browser-mode agent tasks", () => {
  it("lists a demo task with changes to review", async () => {
    const list = await agentTasksApi.list();
    const review = list.find((t) => t.status === "needsReview")!;
    expect(review.steps.some((s) => s.kind === "tests" && s.result.green)).toBe(true);
    const changes = await agentTasksApi.changes(review.id);
    expect(changes.map((c) => c.path)).toEqual(["app/Services/Checkout.php", "tests/Unit/CheckoutTest.php"]);
    expect(changes.every((c) => c.additions > 0)).toBe(true);
    const diff = await agentTasksApi.fileDiff(review.id, "app/Services/Checkout.php");
    expect(diff.after).toContain("public function total(");
    expect(diff.before).not.toContain("public function total(");
  });

  it("replays a run that waits for a command approval", async () => {
    agentTaskMockTiming.scale = 0;
    let latest: AgentTask | null = null;
    await agentTasksApi.onChanged((t) => (latest = t));
    const t = await agentTasksApi.create({ prompt: "Fix it", scope: [], backend: { kind: "builtin" }, tests: { kind: "none" } });
    const pending = await waitFor(() => {
      const cur = latest as AgentTask | null;
      return cur?.id === t.id && cur.steps.find((s) => s.kind === "approval" && s.state === "pending");
    });
    await agentTasksApi.approve(t.id, pending.index, true);
    const done = await waitFor(() => {
      const cur = latest as AgentTask | null;
      return cur?.id === t.id && cur.status === "needsReview" && cur;
    });
    expect(done.steps.find((s) => s.kind === "approval")).toMatchObject({ state: "allowed" });
    expect(done.outcome?.kind).toBe("completed");
    const r = await agentTasksApi.apply(t.id, { mode: "branch", branch: "agent/fix", message: "Fix it" });
    expect(r.created).toBe("agent/fix (a3e81f0)");
    expect((await agentTasksApi.list()).find((x) => x.id === t.id)?.status).toBe("applied");
    agentTaskMockTiming.scale = 1;
  });

  it("refuses tasks when AI is off for the project", async () => {
    setSetting("ai.enabled", false, "project");
    await expect(agentTasksApi.create({ prompt: "x", scope: [], backend: { kind: "builtin" }, tests: { kind: "none" } })).rejects.toThrow(
      "AI features are turned off",
    );
    setSetting("ai.enabled", true, "project");
  });
});
