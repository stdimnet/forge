import { describe, expect, it, vi } from "vitest";
import type { SettingsSnapshot } from "@/ipc/settings";

const m = vi.hoisted(() => ({ replies: [] as ((s: unknown) => void)[] }));
vi.mock("@/ipc/settings", async (orig) => {
  const real = await orig<typeof import("@/ipc/settings")>();
  return {
    ...real,
    settingsApi: { ...real.settingsApi, set: () => Promise.resolve(), getAll: () => new Promise((r) => m.replies.push(r)) },
  };
});

const { getSetting, saveSetting } = await import("./settings");
const snapshot = (size: number) => ({ values: { "editor.font.size": size }, user: {}, project: {}, newProjects: {}, hasProject: false }) as unknown as SettingsSnapshot;

describe("settings refreshes", () => {
  it("apply in order, and an earlier save resolves with the newest state", async () => {
    const first = saveSetting("editor.font.size", 13, "user");
    const second = saveSetting("editor.font.size", 14, "user");
    await new Promise((r) => setTimeout(r, 0));
    m.replies[1](snapshot(14));
    m.replies[0](snapshot(13));
    await first;
    expect(getSetting("editor.font.size")).toBe(14);
    await second;
    expect(getSetting("editor.font.size")).toBe(14);
  });
});
