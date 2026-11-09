import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { GitOverlays } from "./GitOverlays";
import { setInputDialog } from "./overlays";
import { enterConfirms } from "./DialogFrame";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  setInputDialog(null);
  document.body.innerHTML = "";
});

function enter(target: Element): KeyboardEvent {
  const e = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  target.dispatchEvent(e);
  return e;
}

function openConfirm(onOk: () => void) {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <GitOverlays />, host);
  setInputDialog({ title: "Delete Branch", label: "Delete branch topic?", value: "", okLabel: "Delete", noInput: true, checkbox: { label: "Force delete", checked: true }, onOk });
  return host.querySelector(".git-dialog")!;
}

describe("git confirmation dialogs", () => {
  it("do not confirm when Enter presses Cancel or toggles the checkbox", () => {
    const onOk = vi.fn();
    const dialog = openConfirm(onOk);
    const cancel = [...dialog.querySelectorAll("button")].find((b) => b.textContent === "Cancel")!;
    expect(enter(cancel).defaultPrevented).toBe(false);
    expect(enter(dialog.querySelector("input[type=checkbox]")!).defaultPrevented).toBe(false);
    expect(onOk).not.toHaveBeenCalled();
  });

  it("confirm when Enter is pressed elsewhere in the dialog", () => {
    const onOk = vi.fn();
    const dialog = openConfirm(onOk);
    expect(enter(dialog.querySelector(".git-dialog-body")!).defaultPrevented).toBe(true);
    expect(onOk).toHaveBeenCalledOnce();
  });

  it("share the Enter rule with the dialog frame", () => {
    const key = (target: HTMLElement, init: KeyboardEventInit = {}) => {
      const e = new KeyboardEvent("keydown", { key: "Enter", ...init });
      Object.defineProperty(e, "target", { value: target });
      return enterConfirms(e);
    };
    const checkbox = Object.assign(document.createElement("input"), { type: "checkbox" });
    expect(key(document.createElement("button"))).toBe(false);
    expect(key(checkbox)).toBe(false);
    expect(key(document.createElement("input"))).toBe(true);
    expect(key(document.createElement("textarea"))).toBe(false);
    expect(key(document.createElement("textarea"), { ctrlKey: true })).toBe(true);
  });
});
