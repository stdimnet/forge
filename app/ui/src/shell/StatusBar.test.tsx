/**
 * The status bar's caret, separator, encoding, indent and file type items follow the active
 * tab and disappear for view and custom tabs. Switching tabs must never read a tab that is no
 * longer active (#220: `tab()!.cursor` on undefined after Show Local History opened its tab;
 * the exact update order that crashed is covered by the browser-mode action smoke test).
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { render } from "solid-js/web";
import { activateOpenTab, closeAllTabs, closeTab, openCustomTab, openFile, openViewTab, registerFileSystem, updateCursor } from "@/state/editor";
import { StatusBar } from "./StatusBar";

globalThis.ResizeObserver ??= class {
  observe() {}
  disconnect() {}
  unobserve() {}
} as unknown as typeof ResizeObserver;

const files: Record<string, string> = {
  "statusbar-test://a.php": "<?php\necho 1;\n",
  "statusbar-test://b.ts": "const b = 2;\n",
};
registerFileSystem({
  prefix: "statusbar-test://",
  read: async (path) => files[path] ?? "",
  write: async () => {},
});

const tick = () => new Promise((r) => setTimeout(r, 0));
const caret = () => document.querySelector<HTMLElement>('[data-status="caret"]');

let dispose: (() => void) | undefined;
const errors: unknown[] = [];
const onError = (e: ErrorEvent) => errors.push(e.error);

beforeEach(() => {
  errors.length = 0;
  window.addEventListener("error", onError);
  dispose = render(() => <StatusBar />, document.body);
});
afterEach(() => {
  window.removeEventListener("error", onError);
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  closeAllTabs();
});

describe("StatusBar", () => {
  it("shows the caret position of the active text tab and follows tab switches", async () => {
    await openFile("statusbar-test://a.php");
    updateCursor("statusbar-test://a.php", 2, 5);
    await tick();
    expect(caret()?.textContent).toBe("2:5");

    await openFile("statusbar-test://b.ts");
    await tick();
    expect(caret()?.textContent).toBe("1:1");

    activateOpenTab("statusbar-test://a.php");
    await tick();
    expect(caret()?.textContent).toBe("2:5");
    expect(errors).toEqual([]);
  });

  it("hides the file items when a view tab opens and does not throw (#220)", async () => {
    await openFile("statusbar-test://a.php");
    await tick();
    expect(caret()).not.toBeNull();

    expect(() => openViewTab({ path: "local-history://a.php", name: "History", view: "local-history" })).not.toThrow();
    await tick();
    expect(caret()).toBeNull();

    activateOpenTab("statusbar-test://a.php");
    await tick();
    expect(caret()).not.toBeNull();
    expect(errors).toEqual([]);
  });

  it("hides the file items when a custom tab (Show Local History) takes focus (#220)", async () => {
    await openFile("statusbar-test://a.php");
    updateCursor("statusbar-test://a.php", 2, 1);
    await tick();
    expect(caret()?.textContent).toBe("2:1");

    expect(() => openCustomTab({ id: "forge:local-history:a.php", title: "History", component: () => null })).not.toThrow();
    await tick();
    expect(caret()).toBeNull();

    activateOpenTab("statusbar-test://a.php");
    await tick();
    expect(caret()?.textContent).toBe("2:1");
    expect(errors).toEqual([]);
  });

  it("clears the file items when the last tab closes", async () => {
    await openFile("statusbar-test://a.php");
    await tick();
    expect(() => closeTab("statusbar-test://a.php")).not.toThrow();
    await tick();
    expect(caret()).toBeNull();
    expect(errors).toEqual([]);
  });
});
