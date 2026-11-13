import { describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import type { Pipeline, RepoContext } from "@/ipc/forges";

const replies = vi.hoisted(() => ({ repo: [] as ((v: unknown) => void)[], ci: [] as ((v: unknown) => void)[] }));
vi.mock("@/ipc/forges", async (orig) => {
  const real = await orig<typeof import("@/ipc/forges")>();
  return {
    ...real,
    forgesApi: {
      ...real.forgesApi,
      repo: () => new Promise((r) => replies.repo.push(r)),
      ciStatus: () => new Promise((r) => replies.ci.push(r)),
    },
  };
});

const { branchPipeline, context, loadContext, refreshBranchPipeline } = await import("./store");
const ctx = (path: string) => ({ repo: { path } }) as unknown as RepoContext;

describe("code host state", () => {
  it("keeps the newest project's context and CI status", async () => {
    const a = loadContext();
    const b = loadContext();
    replies.repo[1](ctx("team/b"));
    replies.repo[0](ctx("team/a"));
    await Promise.all([a, b]);
    expect(context()?.repo.path).toBe("team/b");

    const older = refreshBranchPipeline();
    const newer = refreshBranchPipeline();
    replies.ci[1]({ id: 2 } as unknown as Pipeline);
    replies.ci[0]({ id: 1 } as unknown as Pipeline);
    await Promise.all([older, newer]);
    expect((branchPipeline() as unknown as { id: number }).id).toBe(2);
  });
});
