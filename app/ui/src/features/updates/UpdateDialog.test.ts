import { describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { CheckResult, UpdateOffer } from "@/ipc/updates";

const m = vi.hoisted(() => ({ replies: [] as ((r: unknown) => void)[] }));
vi.mock("@/ipc/updates", async (orig) => {
  const real = await orig<typeof import("@/ipc/updates")>();
  return { ...real, updatesApi: { ...real.updatesApi, check: () => new Promise((r) => m.replies.push(r)) } };
});

const { openCheckForUpdates, openUpdateOffer, updateDialogState } = await import("./UpdateDialog");

describe("Check for Updates", () => {
  it("does not replace a newer answer with the result of an older check", async () => {
    const check = openCheckForUpdates();
    openUpdateOffer({ version: "0.2.0" } as unknown as UpdateOffer);
    m.replies[0]({ status: "upToDate" } as unknown as CheckResult);
    await check;
    const s = updateDialogState();
    expect(s?.kind === "result" && s.result.status).toBe("available");
  });
});
