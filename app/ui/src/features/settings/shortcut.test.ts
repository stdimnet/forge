import { describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";

describe("the Settings feature", () => {
  it("leaves Ctrl+Alt+S to the keymap (ShowSettings) instead of listening for keys itself", async () => {
    const listen = vi.spyOn(window, "addEventListener");
    await import("./index");
    expect(listen.mock.calls.filter(([type]) => type === "keydown" || type === "keyup")).toEqual([]);
    listen.mockRestore();
  }, 120_000);
});
