import { describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { LaravelLogPage } from "@/ipc/laravel";

const replies = vi.hoisted(() => [] as ((p: unknown) => void)[]);
vi.mock("@/ipc/laravel", async (orig) => {
  const real = await orig<typeof import("@/ipc/laravel")>();
  return { ...real, laravelApi: { ...real.laravelApi, logs: () => new Promise((r) => replies.push(r)) } };
});

const { laravelLogPage, loadLogs } = await import("./LaravelView");
const page = (path: string) => ({ path, files: [], entries: [] }) as unknown as LaravelLogPage;

describe("Laravel logs", () => {
  it("show the newest read, not a slower older one", async () => {
    const first = loadLogs();
    const second = loadLogs();
    replies[1](page("storage/logs/new.log"));
    replies[0](page("storage/logs/old.log"));
    await Promise.all([first, second]);
    expect(laravelLogPage()?.path).toBe("storage/logs/new.log");
  });
});
