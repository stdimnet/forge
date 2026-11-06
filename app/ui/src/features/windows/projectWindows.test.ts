import { beforeEach, describe, expect, it, vi } from "vitest";

const calls = vi.hoisted(() => ({
  setting: "ask" as string,
  answer: "newWindow" as string,
  open: vi.fn(async (_path: string) => undefined),
  openWorkspace: vi.fn(async (_path: string) => true),
  trustBeforeOpen: vi.fn(async (_path: string) => "trust"),
  ask: vi.fn(),
}));

vi.mock("@/ipc/projectWindows", () => ({
  projectWindowsApi: { open: calls.open, list: async () => [], focus: async () => undefined, cycle: async () => null, openEmpty: async () => undefined },
}));
vi.mock("@/state/settings", () => ({ getSetting: () => calls.setting, saveSetting: async () => undefined }));
vi.mock("@/state/workspace", () => ({
  workspace: () => ({ root: "/p/shop", name: "shop" }),
  openWorkspace: calls.openWorkspace,
  trustBeforeOpen: calls.trustBeforeOpen,
  setProjectOpener: () => undefined,
}));
vi.mock("@/features/contextMenu/dialogs", () => ({
  askChoiceWithCheckbox: async (opts: unknown) => {
    calls.ask(opts);
    return { id: calls.answer, checked: false };
  },
}));
vi.mock("@/keymap", () => ({ showBalloon: () => 0 }));

const { openProject } = await import("./projectWindows");

describe("Open Project while a project is open", () => {
  beforeEach(() => {
    calls.open.mockClear();
    calls.openWorkspace.mockClear();
    calls.trustBeforeOpen.mockClear();
    calls.ask.mockClear();
  });

  it("asks This Window / New Window and leaves the trust question to the new window", async () => {
    calls.setting = "ask";
    calls.answer = "newWindow";
    await openProject("/p/untrusted");
    expect(calls.ask).toHaveBeenCalledOnce();
    expect(calls.open).toHaveBeenCalledWith("/p/untrusted");
    // No "Trust and Open Project?" here: the new window asks once it is on screen.
    expect(calls.trustBeforeOpen).not.toHaveBeenCalled();
    expect(calls.openWorkspace).not.toHaveBeenCalled();
  });

  it("opens in this window, which asks the trust question itself", async () => {
    calls.setting = "ask";
    calls.answer = "thisWindow";
    await openProject("/p/untrusted");
    expect(calls.openWorkspace).toHaveBeenCalledWith("/p/untrusted");
    expect(calls.open).not.toHaveBeenCalled();
    expect(calls.trustBeforeOpen).not.toHaveBeenCalled();
  });

  it("follows the Open project in setting without asking", async () => {
    calls.setting = "newWindow";
    await openProject("/p/blog");
    expect(calls.ask).not.toHaveBeenCalled();
    expect(calls.open).toHaveBeenCalledWith("/p/blog");
    await openProject("/p/shop");
    expect(calls.open).toHaveBeenCalledOnce();
  });
});
