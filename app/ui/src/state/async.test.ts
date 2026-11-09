import { describe, expect, it, vi } from "vitest";
import { createRoot } from "solid-js";
import { latestOnly, unsubscribeOnCleanup } from "./async";

describe("unsubscribeOnCleanup", () => {
  it("unsubscribes when the owner is cleaned up before the subscription resolves", async () => {
    const un = vi.fn();
    let resolve!: (u: () => void) => void;
    createRoot((dispose) => {
      unsubscribeOnCleanup(new Promise((r) => (resolve = r)));
      dispose();
    });
    resolve(un);
    await Promise.resolve();
    expect(un).toHaveBeenCalledOnce();
  });

  it("unsubscribes on cleanup after it resolved", async () => {
    const un = vi.fn();
    const dispose = createRoot((dispose) => {
      unsubscribeOnCleanup(Promise.resolve(un));
      return dispose;
    });
    await Promise.resolve();
    expect(un).not.toHaveBeenCalled();
    dispose();
    expect(un).toHaveBeenCalledOnce();
  });
});

describe("latestOnly", () => {
  it("lets only the newest load apply its reply", () => {
    const loads = latestOnly();
    const first = loads.begin();
    const second = loads.begin();
    expect(first()).toBe(false);
    expect(second()).toBe(true);
    loads.invalidate();
    expect(second()).toBe(false);
  });
});
