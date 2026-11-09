/** The shortcut recorder never traps the keyboard and has a role and a name (#217). */
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import { startKeymap } from "@/keymap";
import { loadKeymapCore } from "@/keymap/core";
import { ShortcutField } from "./ShortcutField";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

function field(onChange = vi.fn(), label?: string) {
  dispose = render(() => <ShortcutField value="" onChange={onChange} label={label} />, document.body);
  return { el: document.querySelector<HTMLElement>(".shortcut-field")!, onChange };
}

const key = (el: HTMLElement, init: KeyboardEventInit) => {
  const e = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  el.dispatchEvent(e);
  return e;
};

describe("ShortcutField", () => {
  it("is a named textbox", () => {
    const { el } = field(vi.fn(), "First stroke:");
    expect(el.getAttribute("role")).toBe("textbox");
    expect(el.getAttribute("aria-label")).toBe("First stroke:");
    dispose?.();
    expect(field().el.getAttribute("aria-label")).toBe("Shortcut");
  });

  it("lets Tab and Shift+Tab move the focus instead of recording them", async () => {
    await startKeymap();
    const { el, onChange } = field();
    for (const init of [{ key: "Tab", code: "Tab" }, { key: "Tab", code: "Tab", shiftKey: true }]) {
      const e = key(el, init);
      expect(e.defaultPrevented).toBe(false);
    }
    expect(onChange).not.toHaveBeenCalled();
  });

  it("keeps Escape for the dialog and records other keys", async () => {
    await Promise.all([startKeymap(), loadKeymapCore()]);
    const { el, onChange } = field();
    expect(key(el, { key: "Escape", code: "Escape" }).defaultPrevented).toBe(false);
    expect(key(el, { key: "n", code: "KeyN", ctrlKey: true }).defaultPrevented).toBe(true);
    expect(onChange).toHaveBeenCalledOnce();
    expect(onChange.mock.calls[0][0]).toMatch(/N/);
  });
});
