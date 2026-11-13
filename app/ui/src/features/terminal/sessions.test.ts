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
  const terminals: Terminal[] = [];
  class Terminal {
    options: Record<string, unknown> = {};
    cols = 80;
    rows = 24;
    element = undefined;
    /** The terminal sends `data` to its shell (a typed key, a reply to a query). */
    send: (data: string) => void = () => undefined;
    // The addons are fakes of this class too: only the terminal itself gets a listener.
    onData = (listener: (data: string) => void) => ((this.send = listener), terminals.push(this), any);
    /** Saved output is parsed later; `flushWrites` runs the callbacks like xterm.js does. */
    written: (() => void)[] = [];
    write = (_data: string, done?: () => void) => void (done && this.written.push(done));
    flushWrites = () => this.written.splice(0).forEach((done) => done());
    constructor() {
      return new Proxy(this, {
        get: (t, p) => (p in t ? t[p as keyof Terminal] : any),
      });
    }
  }
  return { any, Terminal, terminals };
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
  write: vi.fn(),
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
    write: (...args: unknown[]) => (api.write(...args), Promise.resolve()),
    resize: () => Promise.resolve(),
  },
}));

const { attachSession, disposeSession, setRestoredHistory } = await import("./sessions");
const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  fake.terminals.length = 0;
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

describe("what the terminal sends before its shell is open", () => {
  /** Start a tab up to the point where `terminal_open` is under way. */
  async function starting(id: string) {
    attachSession(id, document.createElement("div"));
    api.listening[0].resolve(api.unlisten);
    await flush();
    return fake.terminals[fake.terminals.length - 1];
  }

  it("reaches the shell once it is open, in order", async () => {
    const term = await starting("e1");
    // fish asks for the Primary Device Attributes right away; the reply must not be lost.
    term.send("\x1b[?1;2c");
    term.send("ls");
    expect(api.write).not.toHaveBeenCalled();
    api.opening[0].resolve({});
    await flush();
    expect(api.write.mock.calls).toEqual([["e1", "\x1b[?1;2c" + "ls"]]);
    term.send("\r");
    expect(api.write).toHaveBeenLastCalledWith("e1", "\r");
    disposeSession("e1");
  });

  it("is dropped when the tab closes first", async () => {
    const term = await starting("e2");
    term.send("\x1b[?1;2c");
    disposeSession("e2");
    api.opening[0].resolve({});
    await flush();
    expect(api.write).not.toHaveBeenCalled();
  });

  it("leaves out replies to the questions in restored output", async () => {
    setRestoredHistory("e3", "\x1b[0c$ make\r\n");
    const term = await starting("e3");
    // The saved output of the old shell asked too; that reply is not for the new shell.
    term.send("\x1b[?1;2c");
    term.flushWrites();
    term.send("\x1b[?1;2c");
    api.opening[0].resolve({});
    await flush();
    expect(api.write.mock.calls).toEqual([["e3", "\x1b[?1;2c"]]);
    disposeSession("e3");
  });
});
