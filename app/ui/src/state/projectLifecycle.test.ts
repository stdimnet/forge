/** `onProject`: order, error isolation, the `beforeClose` veto and cleanups. */
import { afterEach, describe, expect, it, vi } from "vitest";
import { confirmProjectClose, onProject, runProjectPhase, setObjectionPrompt, type Objection } from "./projectLifecycle";

const a = { name: "a", root: "/p/a" };
const b = { name: "b", root: "/p/b" };

let stops: (() => void)[] = [];
const sub: typeof onProject = (phase, handler, options) => {
  const stop = onProject(phase, handler, options);
  stops.push(stop);
  return stop;
};

afterEach(() => {
  stops.forEach((s) => s());
  stops = [];
  setObjectionPrompt(async () => true);
  vi.restoreAllMocks();
});

describe("project lifecycle handlers", () => {
  it("run by order, then in registration order", () => {
    const log: string[] = [];
    sub("opened", () => void log.push("late"), { order: 10 });
    sub("opened", () => void log.push("first"), { order: -10 });
    sub("opened", () => void log.push("default 1"));
    sub("opened", () => void log.push("default 2"));
    sub("closing", () => void log.push("closing"));
    runProjectPhase("opened", a);
    expect(log).toEqual(["first", "default 1", "default 2", "late"]);
  });

  it("complete synchronously when every handler is synchronous", () => {
    let done = false;
    sub("closing", () => void (done = true));
    expect(runProjectPhase("closing", a)).toBeUndefined();
    expect(done).toBe(true);
  });

  it("await async handlers before the next one runs", async () => {
    const log: string[] = [];
    let release!: () => void;
    sub("opened", () => new Promise<void>((r) => (release = () => (log.push("async"), r()))));
    sub("opened", () => void log.push("after"));
    const running = runProjectPhase("opened", b);
    expect(log).toEqual([]);
    release();
    await running;
    expect(log).toEqual(["async", "after"]);
  });

  it("isolate errors: a throwing or rejecting handler does not stop the others", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const log: string[] = [];
    sub("closing", () => {
      throw new Error("broken");
    }, { name: "broken" });
    sub("closing", () => Promise.reject(new Error("rejected")), { name: "rejects" });
    sub("closing", () => void log.push("healthy"));
    await runProjectPhase("closing", a);
    expect(log).toEqual(["healthy"]);
    expect(warn).toHaveBeenCalledTimes(2);
    expect(String(warn.mock.calls[0][0])).toContain('"broken"');
  });

  it("run the cleanups `opened` returned when that project closes, last one first", () => {
    const log: string[] = [];
    sub("opened", (p) => () => void log.push(`cleanup 1 ${p.name}`));
    sub("opened", (p) => () => void log.push(`cleanup 2 ${p.name}`));
    sub("closing", (p) => void log.push(`closing ${p.name}`));
    runProjectPhase("opened", a);
    expect(log).toEqual([]);
    runProjectPhase("closing", a);
    expect(log).toEqual(["cleanup 2 a", "cleanup 1 a", "closing a"]);
    // Each cleanup runs once.
    runProjectPhase("closing", a);
    expect(log.filter((l) => l.startsWith("cleanup"))).toHaveLength(2);
  });

  it("unsubscribe", () => {
    const handler = vi.fn();
    const stop = sub("ready", handler);
    stop();
    runProjectPhase("ready", a);
    expect(handler).not.toHaveBeenCalled();
  });
});

describe("beforeClose", () => {
  const running: Objection = { source: "terminal", kind: "terminal.running", message: "1 command is running.", items: ["sleep"] };

  it("lets the project close when nobody objects", async () => {
    sub("beforeClose", () => undefined);
    expect(await confirmProjectClose(a)).toBe(true);
  });

  it("stops at the first veto; later handlers are not asked", async () => {
    const later = vi.fn();
    sub("beforeClose", () => false, { order: -1 });
    sub("beforeClose", later);
    expect(await confirmProjectClose(a)).toBe(false);
    expect(later).not.toHaveBeenCalled();
  });

  it("waits for an async answer (a dialog)", async () => {
    let answer!: (ok: boolean) => void;
    sub("beforeClose", () => new Promise<boolean>((r) => (answer = r)));
    const pending = confirmProjectClose(a);
    answer(false);
    expect(await pending).toBe(false);
  });

  it("does not veto when a handler throws", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    sub("beforeClose", () => {
      throw new Error("broken");
    });
    expect(await confirmProjectClose(a)).toBe(true);
  });

  it("asks about the objections no handler claimed", async () => {
    const prompt = vi.fn(async () => false);
    setObjectionPrompt(prompt);
    expect(await confirmProjectClose(a, [running])).toBe(false);
    expect(prompt).toHaveBeenCalledWith(a, [running]);

    prompt.mockClear();
    let claimed: Objection[] = [];
    sub("beforeClose", (_p, ctx) => void (claimed = ctx.claim("terminal.running")));
    expect(await confirmProjectClose(a, [running])).toBe(true);
    expect(claimed).toEqual([running]);
    expect(prompt).not.toHaveBeenCalled();
  });
});
