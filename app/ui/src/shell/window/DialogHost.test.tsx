import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { JSX } from "solid-js";

// The desktop app: `DialogWindowApp` talks to the backend, which the registered mocks play.
vi.mock("@/ipc", async (orig) => ({ ...(await orig<typeof import("@/ipc")>()), isTauri: () => true }));
vi.mock("@/state/workspace", () => ({
  restoreWorkspace: () => invoke("workspace_current"),
  openWorkspace: async () => undefined,
}));
vi.mock("./mode", async (orig) => ({
  ...(await orig<typeof import("./mode")>()),
  windowLabel: async () => "dialog-t-dialog",
}));
vi.mock("./chrome", () => ({ trackMaximized: () => undefined }));
vi.mock("./WindowFrame", () => ({
  WindowFrame: (props: { title: string; children: JSX.Element }) => <div data-title={props.title}>{props.children}</div>,
}));

const { emitMock, invoke, registerMock } = await import("@/ipc");
const { registerDialog } = await import("./dialogs");
const { DialogWindowApp } = await import("./DialogHost");

/** Commands the dialog window sent, in order. */
let calls: string[] = [];
let dispose: (() => void) | undefined;
/** What `dialog_window_parked` answers: the dialog was requested while the window loaded. */
let requestedMeanwhile: unknown = null;

for (const cmd of ["workspace_current", "dialog_window_ready", "t_prepare"]) {
  registerMock(cmd, () => void calls.push(cmd));
}
registerMock("dialog_window_parked", () => {
  calls.push("dialog_window_parked");
  return requestedMeanwhile;
});

registerDialog<{ page?: string }>({
  id: "t-dialog",
  title: (props) => `Dialog ${props?.page ?? ""}`.trim(),
  width: 600,
  height: 400,
  component: (props) => <p class="t-content">page: {props.props.page ?? "none"}</p>,
  prepare: () => invoke("t_prepare"),
});

function mount(boot: { props?: unknown; parked?: boolean }): HTMLElement {
  const host = document.createElement("div");
  dispose = render(() => <DialogWindowApp boot={{ id: "t-dialog", parent: "main", ...boot }} />, host);
  return host;
}

const count = (cmd: string) => calls.filter((c) => c === cmd).length;
const settled = () => new Promise((r) => setTimeout(r, 20));

afterEach(() => {
  dispose?.();
  dispose = undefined;
  calls = [];
  requestedMeanwhile = null;
  vi.unstubAllGlobals();
});

describe("a dialog window", () => {
  it("reports ready although a hidden window runs no animation frames (#244)", async () => {
    // The window is hidden until it reports ready, and a hidden window never runs an
    // animation frame: waiting for one left the dialog to the backend's 10 s fallback.
    const frame = vi.fn(() => 0);
    vi.stubGlobal("requestAnimationFrame", frame);
    const host = mount({ props: { page: "keymap" } });
    await vi.waitFor(() => expect(count("dialog_window_ready")).toBe(1));
    expect(host.querySelector(".t-content")?.textContent).toBe("page: keymap");
    expect(frame).not.toHaveBeenCalled();
    expect(count("dialog_window_parked")).toBe(0);
  });
});

describe("a dialog window built ahead of time", () => {
  it("loads, reports and stays hidden until its dialog is requested", async () => {
    const host = mount({ parked: true });
    await vi.waitFor(() => expect(count("dialog_window_parked")).toBe(1));
    await settled();
    // Loaded what the dialog shows, but nothing is on screen and the window is not shown.
    expect(count("t_prepare")).toBe(1);
    expect(host.querySelector(".t-content")).toBeNull();
    expect(count("dialog_window_ready")).toBe(0);
  });

  it("shows the requested dialog with its props after refreshing only what it shows", async () => {
    const host = mount({ parked: true });
    await vi.waitFor(() => expect(count("dialog_window_parked")).toBe(1));
    calls = [];
    // Requests for other windows are ignored.
    emitMock("dialog://activate", { window: "dialog-other", props: { page: "other" } });
    await settled();
    expect(calls).toEqual([]);

    emitMock("dialog://activate", { window: "dialog-t-dialog", props: { page: "plugins" } });
    await vi.waitFor(() => expect(count("dialog_window_ready")).toBe(1));
    expect(host.querySelector(".t-content")?.textContent).toBe("page: plugins");
    expect(host.querySelector("[data-title]")?.getAttribute("data-title")).toBe("Dialog plugins");
    // Opening costs the project, the dialog's data and the ready report: no second boot.
    expect(calls).toEqual(["workspace_current", "t_prepare", "dialog_window_ready"]);
  });

  it("shows a dialog that was requested while the window loaded", async () => {
    requestedMeanwhile = { window: "dialog-t-dialog", props: { page: "appearance" } };
    const host = mount({ parked: true });
    await vi.waitFor(() => expect(count("dialog_window_ready")).toBe(1));
    expect(host.querySelector(".t-content")?.textContent).toBe("page: appearance");
    expect(count("dialog_window_parked")).toBe(1);
  });
});
