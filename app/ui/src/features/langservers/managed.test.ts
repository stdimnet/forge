import { describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { ServerRow } from "@/ipc/langservers";

const api = vi.hoisted(() => ({ order: [] as string[], output: undefined as undefined | ((l: unknown) => void) }));
vi.mock("@/ipc/langservers", async (orig) => {
  const real = await orig<typeof import("@/ipc/langservers")>();
  // Subscribing takes a moment, like Tauri's `listen`.
  const later = <T,>(v: T) => new Promise<T>((r) => setTimeout(() => r(v), 5));
  return {
    ...real,
    langserversApi: {
      ...real.langserversApi,
      onInstallProgress: () => later(() => {}).then((u) => (api.order.push("progress"), u)),
      onInstallOutput: (cb: (l: unknown) => void) => later(() => {}).then((u) => (api.order.push("output"), (api.output = cb), u)),
      install: async () => {
        api.order.push("install");
        return "cancelled";
      },
    },
  };
});

const { installManaged } = await import("./managed");

describe("managed installs", () => {
  it("listen to the installer's events before the install starts", async () => {
    const row = { id: "intelephense", name: "Intelephense", install: [{ index: 0, managed: true, runnable: true }] } as unknown as ServerRow;
    await installManaged(row);
    await new Promise((r) => setTimeout(r, 20));
    expect(api.order.indexOf("install")).toBeGreaterThan(api.order.indexOf("output"));
    expect(api.order.indexOf("install")).toBeGreaterThan(api.order.indexOf("progress"));
  });
});
