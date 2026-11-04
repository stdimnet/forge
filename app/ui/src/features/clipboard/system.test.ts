import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const balloon = vi.fn();
vi.mock("@/keymap", () => ({ showBalloon: (...args: unknown[]) => balloon(...args) }));
const tauriInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => tauriInvoke(...args) }));

import "@/mock/clipboardMock";
import { readClipboard, writeClipboard, writeClipboardText } from "./system";

function stubClipboard(clipboard: Partial<Clipboard> | undefined): void {
  Object.defineProperty(navigator, "clipboard", { value: clipboard, configurable: true });
}

beforeEach(() => {
  balloon.mockReset();
  tauriInvoke.mockReset();
});

afterEach(() => {
  stubClipboard(undefined);
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("browser mode (navigator.clipboard)", () => {
  it("reads and writes the browser clipboard", async () => {
    let stored = "";
    stubClipboard({ readText: async () => stored, writeText: async (t: string) => void (stored = t) });
    expect(await writeClipboard("echo hi\nls")).toBe(true);
    expect(await readClipboard()).toBe("echo hi\nls");
    expect(balloon).not.toHaveBeenCalled();
  });

  it("shows a balloon instead of failing silently", async () => {
    stubClipboard({
      readText: async () => Promise.reject(new Error("Read permission denied")),
      writeText: async () => Promise.reject(new Error("Write permission denied")),
    });
    expect(await readClipboard()).toBeNull();
    expect(balloon).toHaveBeenLastCalledWith(expect.any(String), "Read permission denied", 5000, undefined, { type: "error" });
    expect(await writeClipboard("x")).toBe(false);
    expect(balloon).toHaveBeenLastCalledWith(expect.any(String), "Write permission denied", 5000, undefined, { type: "error" });
    await expect(writeClipboardText("x")).rejects.toThrow("Write permission denied");
  });

  it("reports a browser without clipboard access", async () => {
    stubClipboard(undefined);
    expect(await readClipboard()).toBeNull();
    expect(balloon.mock.calls[0][1]).toMatch(/no clipboard access/);
  });

  it("stays quiet when asked to and gives up after the timeout", async () => {
    stubClipboard({ readText: async () => Promise.reject(new Error("denied")) });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    expect(await readClipboard({ quiet: true })).toBeNull();
    expect(balloon).not.toHaveBeenCalled();
    warn.mockRestore();
    stubClipboard({ readText: () => new Promise<string>(() => undefined) });
    expect(await readClipboard({ timeoutMs: 10 })).toBeNull();
  });
});

describe("desktop mode (native clipboard)", () => {
  it("reads through the Rust command, not navigator.clipboard", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const readText = vi.fn();
    stubClipboard({ readText });
    tauriInvoke.mockResolvedValueOnce("from rust");
    expect(await readClipboard()).toBe("from rust");
    expect(tauriInvoke).toHaveBeenCalledWith("clipboard_read_text", {});
    expect(readText).not.toHaveBeenCalled();

    tauriInvoke.mockResolvedValueOnce(undefined);
    expect(await writeClipboard("copied")).toBe(true);
    expect(tauriInvoke).toHaveBeenLastCalledWith("clipboard_write_text", { text: "copied" });

    tauriInvoke.mockRejectedValueOnce("Cannot read the clipboard: no display");
    expect(await readClipboard()).toBeNull();
    expect(balloon).toHaveBeenLastCalledWith(expect.any(String), "Cannot read the clipboard: no display", 5000, undefined, { type: "error" });
  });
});
