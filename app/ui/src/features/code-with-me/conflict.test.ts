import { describe, expect, it, vi } from "vitest";
import { conflictHandler, keepLabel, loadLabel, type ConflictUi } from "./conflict";

const tick = () => new Promise((r) => setTimeout(r, 0));

function fakeUi(role: "host" | "guest", answers: (string | null)[]) {
  const balloons = new Map<number, { title: string; detail: string; actions?: { label: string; run: () => void }[] }>();
  let seq = 0;
  const ui: ConflictUi = {
    role: () => role,
    ask: vi.fn(async () => answers.shift() ?? null),
    resolve: vi.fn(async () => undefined),
    balloon: (title, detail, actions) => {
      balloons.set(++seq, { title, detail, actions });
      return seq;
    },
    dismiss: (id) => void balloons.delete(id),
    showDiff: vi.fn(async () => undefined),
    docName: () => "OrderController.php",
    hostName: () => "Robert",
    error: vi.fn(),
  };
  return { ui, balloons };
}

const opened = (docId: number, path = "app/Http/Controllers/OrderController.php") => ({ docId, conflict: { open: true, path } });

describe("File Cache Conflict", () => {
  it("asks the host and sends the answer", async () => {
    const { ui } = fakeUi("host", ["load"]);
    const c = conflictHandler(ui);
    c.handle(opened(3));
    // A repeated report while the dialog is open asks only once.
    c.handle(opened(3));
    await tick();
    expect(ui.ask).toHaveBeenCalledTimes(1);
    const [title, message, choices] = (ui.ask as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(title).toBe("File Cache Conflict");
    expect(message).toBe("Changes have been made to 'app/Http/Controllers/OrderController.php' in memory and on disk.");
    // The last choice is the default (Enter), as in PHPStorm.
    expect(choices.map((x: { label: string }) => x.label)).toEqual(["Show Difference", keepLabel(), loadLabel()]);
    expect(ui.resolve).toHaveBeenCalledWith(3, true);
    expect(c.openConflicts()).toEqual([]);
  });

  it("asks for one file after the other", async () => {
    const { ui } = fakeUi("host", ["keep", "load"]);
    const c = conflictHandler(ui);
    c.handle(opened(1, "a.php"));
    c.handle(opened(2, "b.php"));
    await tick();
    await tick();
    expect(ui.resolve).toHaveBeenNthCalledWith(1, 1, false);
    expect(ui.resolve).toHaveBeenNthCalledWith(2, 2, true);
  });

  it("keeps the choice in a sticky balloon when the dialog is closed or the diff is shown", async () => {
    const { ui, balloons } = fakeUi("host", ["diff"]);
    const c = conflictHandler(ui);
    c.handle(opened(4, "routes/web.php"));
    await tick();
    expect(ui.showDiff).toHaveBeenCalledWith(4, "routes/web.php");
    expect(ui.resolve).not.toHaveBeenCalled();
    const [balloon] = [...balloons.values()];
    expect(balloon.detail).toBe("Changes have been made to web.php in memory and on disk.");
    expect(balloon.actions?.map((a) => a.label)).toEqual([loadLabel(), keepLabel()]);
    balloon.actions![1].run();
    await tick();
    expect(ui.resolve).toHaveBeenCalledWith(4, false);
    expect(balloons.size).toBe(0);
  });

  it("drops the choice when the conflict ended elsewhere", async () => {
    const { ui, balloons } = fakeUi("host", [null]);
    const c = conflictHandler(ui);
    c.handle(opened(5));
    await tick();
    expect(balloons.size).toBe(1);
    c.handle({ docId: 5, conflict: { open: false, path: "" } });
    expect(balloons.size).toBe(0);
    expect(c.openConflicts()).toEqual([]);
  });

  it("tells guests that the host is resolving it", async () => {
    const { ui, balloons } = fakeUi("guest", []);
    const c = conflictHandler(ui);
    c.handle({ docId: 7, conflict: { open: true, path: "" } });
    await tick();
    expect(ui.ask).not.toHaveBeenCalled();
    expect([...balloons.values()][0].detail).toBe(
      "OrderController.php changed on Robert's disk while it has unsaved changes. Robert is choosing which version to keep; saving waits until then.",
    );
    c.handle({ docId: 7, conflict: { open: false, path: "" } });
    expect(balloons.size).toBe(0);
  });
});
