import { describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { ServiceDetails } from "@/ipc/services";

vi.hoisted(() => {
  // jsdom has no canvas (xterm.js measures with one when its module loads).
  HTMLCanvasElement.prototype.getContext = (() => null) as typeof HTMLCanvasElement.prototype.getContext;
});

const mocks = vi.hoisted(() => ({
  attachSession: vi.fn(),
  disposeSession: vi.fn(),
  resolveCommand: undefined as undefined | ((cmd: unknown) => void),
}));

vi.mock("@/features/terminal/sessions", async (orig) => ({
  ...(await orig<typeof import("@/features/terminal/sessions")>()),
  attachSession: mocks.attachSession,
  disposeSession: mocks.disposeSession,
  fitSession: () => {},
}));
vi.mock("@/ipc/services", async (orig) => {
  const real = await orig<typeof import("@/ipc/services")>();
  return {
    ...real,
    servicesApi: { ...real.servicesApi, terminal: () => new Promise((resolve) => (mocks.resolveCommand = resolve)) },
  };
});

const { AttachedConsole } = await import("./DetailsPane");

describe("Attached Console", () => {
  it("does not attach when it is closed while its command loads", async () => {
    const host = document.createElement("div");
    const dispose = render(() => <AttachedConsole d={{ nodeId: "docker:web" } as ServiceDetails} />, host);
    await Promise.resolve();
    dispose();
    expect(mocks.disposeSession).toHaveBeenCalledOnce();
    mocks.resolveCommand!({ program: "docker", args: ["attach", "web"], env: [], onHost: true });
    await new Promise((r) => setTimeout(r, 0));
    expect(mocks.attachSession).not.toHaveBeenCalled();
  });
});
