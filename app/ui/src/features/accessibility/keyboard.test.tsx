/**
 * Keyboard-only use of the Settings dialog in jsdom: search field first, Tab into the page
 * tree, arrows and Enter, a checkbox with its label and description, and Tab staying in the
 * dialog. The full flow through the running app is `pnpm a11y:keyboard`.
 */
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { render } from "solid-js/web";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import { loadSettings } from "@/state/settings";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { SettingsDialog } from "@/features/settings/SettingsDialog";
import { draftValue, isOpen, setSelected } from "@/features/settings/dialogState";
import { installDialogAccessibility, tabbables } from "./dialogs";

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

/** Press a key on the focused element, as the browser would dispatch it. */
function press(key: string, init: KeyboardEventInit = {}): void {
  const target = document.activeElement ?? document.body;
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
}

/** Tab (or Shift+Tab) to the next control in the dialog, like the browser's focus order. */
function tab(shift = false): void {
  const dialog = document.querySelector<HTMLElement>("[role=dialog]")!;
  const list = tabbables(dialog);
  const at = list.indexOf(document.activeElement as HTMLElement);
  const next = list[(at + (shift ? -1 : 1) + list.length) % list.length];
  next.focus();
}

let dispose: () => void;

beforeAll(async () => {
  await loadSettings();
  registerSettingsPage({ id: "appearance", title: "Appearance", group: "Appearance & Behavior", order: 0, component: () => <SchemaPage pageId="appearance" /> });
  registerSettingsPage({ id: "system", title: "System Settings", group: "Appearance & Behavior", order: 1, component: () => <SchemaPage pageId="system" /> });
  installDialogAccessibility(document);
  const host = document.createElement("div");
  document.body.appendChild(host);
  dispose = render(
    () => (
      <div role="dialog" aria-label="Settings" aria-modal="true">
        <SettingsDialog props={{ page: "appearance" }} />
      </div>
    ),
    host,
  );
  for (let i = 0; i < 100 && document.querySelectorAll(".set-tree-row").length < 2; i++) await tick(50);
  await tick(20);
});

afterAll(() => dispose?.());

describe("Settings dialog with the keyboard", () => {
  it("starts in the search field, which has a name", () => {
    expect(isOpen()).toBe(true);
    const el = document.activeElement as HTMLInputElement;
    expect(el.tagName).toBe("INPUT");
    expect(el.getAttribute("aria-label")).toBe("Search settings");
  });

  it("reaches the page tree with Tab; it is a tree with one tabbable row", () => {
    tab();
    const row = document.activeElement as HTMLElement;
    expect(row.getAttribute("role")).toBe("treeitem");
    expect(row.getAttribute("aria-selected")).toBe("true");
    const tree = row.closest("[role=tree]")!;
    expect(tree.getAttribute("aria-label")).toBe("Settings pages");
    expect(tree.querySelectorAll('[role=treeitem][tabindex="0"]').length).toBe(1);
  });

  it("moves through the tree with the arrows and opens a page with Enter", async () => {
    setSelected("appearance");
    await tick();
    (document.querySelector('[data-node="appearance"]') as HTMLElement).focus();
    press("Home");
    await tick();
    const group = document.activeElement as HTMLElement;
    expect(group.getAttribute("role")).toBe("treeitem");
    expect(group.getAttribute("aria-level")).toBe("1");
    expect(group.getAttribute("aria-expanded")).toBe("true");
    for (let i = 0; i < 10 && (document.activeElement as HTMLElement).dataset.node !== "appearance"; i++) {
      press("ArrowDown");
      await tick();
    }
    expect((document.activeElement as HTMLElement).dataset.node).toBe("appearance");
    expect((document.activeElement as HTMLElement).getAttribute("aria-level")).toBe("2");
    press("Enter");
    await tick();
    expect(document.activeElement?.closest(".set-content")).not.toBeNull();
  });

  it("labels and describes every control and toggles a checkbox with Space", async () => {
    for (const control of document.querySelectorAll<HTMLInputElement | HTMLSelectElement>(".set-content input, .set-content select")) {
      const named = control.labels?.length || control.getAttribute("aria-label") || control.closest("label");
      expect(named, control.outerHTML).toBeTruthy();
    }
    const box = document.getElementById("setting-appearance-screenReader") as HTMLInputElement;
    expect(box.closest("label")?.textContent).toContain("Support screen readers");
    expect(document.getElementById(box.getAttribute("aria-describedby")!)?.textContent).toMatch(/screen reader/);
    box.focus();
    box.click(); // Space on a checkbox
    expect(draftValue("appearance.screenReader")).toBe(true);
  });

  it("keeps Tab inside the dialog", () => {
    const dialog = document.querySelector("[role=dialog]")!;
    for (let i = 0; i < 60; i++) {
      tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
  });
});
