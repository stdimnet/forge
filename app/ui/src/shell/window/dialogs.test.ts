import { afterEach, describe, expect, it, vi } from "vitest";
import { emitMock, registerMock } from "@/ipc";
import {
  closeDialog,
  closeOwnedDialog,
  dialogDef,
  instanceId,
  inPageDialogs,
  isDialogOpen,
  notifyDialogOpener,
  onDialogCancel,
  openDialogWindow,
  registerDialog,
  requestCloseDialog,
  setDialogWindowMode,
} from "./dialogs";

const Empty = () => null;

function dialog(id: string, extra: Partial<Parameters<typeof registerDialog>[0]> = {}) {
  const onClosed = vi.fn();
  const onMessage = vi.fn();
  registerDialog({ id, title: "Test", width: 600, height: 400, component: Empty, onClosed, onMessage, ...extra });
  return { onClosed, onMessage };
}

afterEach(() => setDialogWindowMode(false));

describe("in-page dialogs (browser mode)", () => {
  it("opens, closes with a result and runs onClosed in the opener", async () => {
    const { onClosed } = dialog("t-basic");
    const done = openDialogWindow<{ ok: boolean }>("t-basic", { page: "a" });
    expect(inPageDialogs().map((d) => d.id)).toContain("t-basic");
    expect(isDialogOpen("t-basic")).toBe(true);
    closeDialog("t-basic", { ok: true });
    await expect(done).resolves.toEqual({ ok: true });
    expect(onClosed).toHaveBeenCalledWith({ ok: true }, { page: "a" });
    expect(inPageDialogs().some((d) => d.id === "t-basic")).toBe(false);
    expect(isDialogOpen("t-basic")).toBe(false);
  });

  it("opens a dialog only once", async () => {
    dialog("t-once");
    const a = openDialogWindow("t-once");
    const b = openDialogWindow("t-once");
    expect(b).toBe(a);
    expect(inPageDialogs().filter((d) => d.id === "t-once")).toHaveLength(1);
    closeDialog("t-once");
    await expect(a).resolves.toBeNull();
  });

  it("cancels through the dialog's handler", async () => {
    dialog("t-cancel");
    const done = openDialogWindow("t-cancel");
    let busy = true;
    const off = onDialogCancel("t-cancel", () => !busy && closeDialog("t-cancel"));
    requestCloseDialog("t-cancel");
    expect(isDialogOpen("t-cancel")).toBe(true);
    busy = false;
    requestCloseDialog("t-cancel");
    await expect(done).resolves.toBeNull();
    off();
  });

  it("cancels without a handler", async () => {
    dialog("t-plain");
    const done = openDialogWindow("t-plain");
    requestCloseDialog("t-plain");
    await expect(done).resolves.toBeNull();
  });

  it("delivers messages to the opener", () => {
    const { onMessage } = dialog("t-msg");
    void openDialogWindow("t-msg");
    notifyDialogOpener("t-msg", { saved: ["a"] });
    expect(onMessage).toHaveBeenCalledWith({ saved: ["a"] }, {});
    closeDialog("t-msg");
  });

  it("rejects unknown dialogs", async () => {
    await expect(openDialogWindow("t-nope")).rejects.toThrow(/unknown dialog/);
  });

  it("opens several windows of one definition, not modal ones without a backdrop", async () => {
    const { onClosed } = dialog("t-multi", { modal: false });
    const a = instanceId("t-multi", "a.php");
    const b = instanceId("t-multi", "b");
    expect(a).toBe("t-multi--a_php");
    expect(dialogDef(a)?.id).toBe("t-multi");
    void openDialogWindow(a, { n: 1 });
    void openDialogWindow(b, { n: 2 });
    const open = inPageDialogs().filter((d) => d.id.startsWith("t-multi--"));
    expect(open.map((d) => [d.id, d.modeless])).toEqual([
      [a, true],
      [b, true],
    ]);
    closeDialog(b, "done");
    expect(onClosed).toHaveBeenCalledWith("done", { n: 2 });
    expect(isDialogOpen(a)).toBe(true);
    await closeOwnedDialog(a);
    expect(isDialogOpen(a)).toBe(false);
    expect(onClosed).toHaveBeenLastCalledWith(null, { n: 1 });
  });
});

describe("dialog windows (desktop)", () => {
  it("asks the backend for a window and resolves when it reports the dialog closed", async () => {
    const { onClosed, onMessage } = dialog("t-window", { title: (p) => `Edit ${(p as { name?: string }).name}`, minWidth: 500 });
    setDialogWindowMode(true);
    const requests: unknown[] = [];
    registerMock("dialog_window_open", (args) => void requests.push(args.request));

    const done = openDialogWindow("t-window", { name: "x" });
    await vi.waitFor(() => expect(requests).toHaveLength(1));
    expect(requests[0]).toEqual({
      id: "t-window",
      title: "Edit x",
      width: 600,
      height: 400,
      minWidth: 500,
      minHeight: undefined,
      props: { name: "x" },
      modal: true,
    });
    expect(inPageDialogs().some((d) => d.id === "t-window")).toBe(false);
    expect(isDialogOpen("t-window")).toBe(true);

    // Closing from the opener does nothing: the dialog window closes itself.
    closeDialog("t-window", "ignored");
    expect(isDialogOpen("t-window")).toBe(true);

    emitMock("dialog://message", { window: "main", id: "t-window", message: "applied" });
    expect(onMessage).toHaveBeenCalledWith("applied", { name: "x" });
    // Events for other windows are ignored.
    emitMock("dialog://closed", { window: "dialog-other", id: "t-window", result: 1 });
    expect(isDialogOpen("t-window")).toBe(true);

    emitMock("dialog://closed", { window: "main", id: "t-window", result: { ok: 1 } });
    await expect(done).resolves.toEqual({ ok: 1 });
    expect(onClosed).toHaveBeenCalledWith({ ok: 1 }, { name: "x" });
    expect(isDialogOpen("t-window")).toBe(false);
  });

  it("gives up when the window cannot be created", async () => {
    dialog("t-fail");
    setDialogWindowMode(true);
    registerMock("dialog_window_open", () => {
      throw new Error("no window");
    });
    const spy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    await expect(openDialogWindow("t-fail")).resolves.toBeNull();
    expect(isDialogOpen("t-fail")).toBe(false);
    spy.mockRestore();
  });
});
