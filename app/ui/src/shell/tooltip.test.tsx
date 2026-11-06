import { afterEach, describe, expect, it } from "vitest";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { tooltip } from "./tooltip";
import { claimPopup } from "./popupGroup";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

const pointer = (el: Element, type: string) => el.dispatchEvent(new Event(type, { bubbles: type !== "pointerleave" }));

describe("tooltip", () => {
  it("hides the tooltip from a press until the pointer leaves, keeping the accessible name", () => {
    dispose = render(() => <button {...tooltip(() => "Code With Me")} />, document.body);
    const btn = document.querySelector("button")!;
    expect(btn.getAttribute("title")).toBe("Code With Me");
    expect(btn.getAttribute("aria-label")).toBe("Code With Me");
    pointer(btn, "pointerdown");
    expect(btn.hasAttribute("title")).toBe(false);
    expect(btn.getAttribute("aria-label")).toBe("Code With Me");
    pointer(btn, "pointerleave");
    expect(btn.getAttribute("title")).toBe("Code With Me");
  });

  it("shows no tooltip while the control's popup is open", () => {
    const [open, setOpen] = createSignal(false);
    dispose = render(() => <button {...tooltip(() => "Code With Me", { open })} />, document.body);
    const btn = document.querySelector("button")!;
    setOpen(true);
    expect(btn.hasAttribute("title")).toBe(false);
    expect(btn.getAttribute("aria-label")).toBe("Code With Me");
    setOpen(false);
    expect(btn.getAttribute("title")).toBe("Code With Me");
  });

  it("leaves the name to the visible text with label: false", () => {
    dispose = render(() => <button {...tooltip(() => "Details", { label: false })}>Name</button>, document.body);
    const btn = document.querySelector("button")!;
    expect(btn.getAttribute("title")).toBe("Details");
    expect(btn.hasAttribute("aria-label")).toBe(false);
  });
});

describe("claimPopup", () => {
  it("closes the popup that was open before", () => {
    const closed: string[] = [];
    const releaseA = claimPopup(() => closed.push("a"));
    claimPopup(() => closed.push("b"));
    expect(closed).toEqual(["a"]);
    // A late release of the first popup does not forget the second one.
    releaseA();
    claimPopup(() => closed.push("c"));
    expect(closed).toEqual(["a", "b"]);
  });

  it("does not close a popup that was released", () => {
    const closed: string[] = [];
    const release = claimPopup(() => closed.push("a"));
    release();
    const releaseB = claimPopup(() => closed.push("b"));
    expect(closed).toEqual([]);
    releaseB();
  });
});
