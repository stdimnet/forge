import { afterEach, describe, expect, it, vi } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";

// xterm.js needs a canvas jsdom does not have; these tests only look at the session's lifecycle.
const fake = vi.hoisted(() => {
  // Any property is a no-op function that returns the same kind of object (disposables, addons).
  const any: object = new Proxy(function () {}, {
    get: (_t, p) => (p === "then" ? undefined : any),
    apply: () => any,
    construct: () => any,
  });
  class Terminal {
    options: Record<string, unknown> = {};
    cols = 80;
    rows = 24;
    element = undefined;
    constructor() {
      return new Proxy(this, {
        get: (t, p) => (p in t ? t[p as keyof Terminal] : any),
      });
    }
  }
  return { any, Terminal };
});
vi.mock("@xterm/xterm", () => ({ Terminal: fake.Terminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: fake.Terminal }));
vi.mock("@xterm/addon-web-links", () => ({ WebLinksAddon: fake.Terminal }));
vi.mock("@xterm/addon-webgl", () => ({ WebglAddon: fake.Terminal }));
vi.mock("@xterm/addon-unicode-graphemes", () => ({
  UnicodeGraphemesAddon: fake.Terminal,
}));
vi.mock("@xterm/addon-unicode11", () => ({ Unicode11Addon: fake.Terminal }));

const api = vi.hoisted(() => ({
  listening: [] as { resolve: (u: () => void) => void }[],
  opening: [] as { resolve: (v: unknown) => void }[],
  open: vi.fn(),
  close: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock("@/ipc/terminal", async (orig) => ({
  ...(await orig<typeof import("@/ipc/terminal")>()),
  terminalApi: {
    onEvent: () =>
      new Promise<() => void>((resolve) => api.listening.push({ resolve })),
    open: (...args: unknown[]) => {
      api.open(...args);
      return new Promise((resolve) => api.opening.push({ resolve }));
    },
    close: (id: string) => (api.close(id), Promise.resolve()),
    write: () => Promise.resolve(),
    resize: () => Promise.resolve(),
  },
}));

const { attachSession, disposeSession } = await import("./sessions");
const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  api.listening.length = 0;
  api.opening.length = 0;
  vi.clearAllMocks();
});

describe("terminal sessions closed while they start", () => {
  it("stop listening and open nothing when closed before the listener is ready", async () => {
    attachSession("t1", document.createElement("div"));
    disposeSession("t1");
    api.listening[0].resolve(api.unlisten);
    await flush();
    expect(api.unlisten).toHaveBeenCalledOnce();
    expect(api.open).not.toHaveBeenCalled();
  });

  it("close the shell that opened after the tab was closed", async () => {
    attachSession("t2", document.createElement("div"));
    api.listening[0].resolve(api.unlisten);
    await flush();
    expect(api.open).toHaveBeenCalledOnce();
    disposeSession("t2");
    // Not open yet: nothing to close, but the listener is gone.
    expect(api.close).not.toHaveBeenCalled();
    expect(api.unlisten).toHaveBeenCalledOnce();
    api.opening[0].resolve({});
    await flush();
    expect(api.close).toHaveBeenCalledWith("t2");
  });

  it("close an open shell when the tab closes", async () => {
    attachSession("t3", document.createElement("div"));
    api.listening[0].resolve(api.unlisten);
    await flush();
    api.opening[0].resolve({});
    await flush();
    disposeSession("t3");
    expect(api.close).toHaveBeenCalledWith("t3");
  });
});
