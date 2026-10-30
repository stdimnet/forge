import { describe, expect, it, vi } from "vitest";
import { navigateItems, nextIndex } from "./a11y";

describe("popup keyboard navigation", () => {
  it("moves around the items and wraps", () => {
    expect(nextIndex(-1, 3, "ArrowDown")).toBe(0);
    expect(nextIndex(-1, 3, "ArrowUp")).toBe(2);
    expect(nextIndex(2, 3, "ArrowDown")).toBe(0);
    expect(nextIndex(0, 3, "ArrowUp")).toBe(2);
    expect(nextIndex(1, 3, "Home")).toBe(0);
    expect(nextIndex(1, 3, "End")).toBe(2);
    expect(nextIndex(1, 3, "a")).toBeNull();
    expect(nextIndex(-1, 0, "ArrowDown")).toBeNull();
  });

  it("focuses and activates data-nav items but leaves native controls alone", () => {
    const box = document.createElement("div");
    box.tabIndex = -1;
    box.innerHTML = `<input type="radio"><div data-nav tabindex="-1">Copy Session Link</div><div data-nav tabindex="-1" aria-disabled="true">Disabled</div><div data-nav tabindex="-1">End Session</div>`;
    document.body.append(box);
    const [radio, copy, , end] = [...box.children] as HTMLElement[];
    const key = (k: string, target: HTMLElement = box) => {
      const e = new KeyboardEvent("keydown", { key: k });
      Object.defineProperty(e, "target", { value: target });
      return navigateItems(e, box);
    };
    box.focus();
    expect(key("ArrowDown")).toBe(true);
    expect(document.activeElement).toBe(copy);
    // The disabled item is skipped.
    key("ArrowDown", copy);
    expect(document.activeElement).toBe(end);
    const clicked = vi.fn();
    end.addEventListener("click", clicked);
    expect(key("Enter", end)).toBe(true);
    expect(clicked).toHaveBeenCalledOnce();
    // Arrow keys on a radio button stay the radio group's.
    expect(key("ArrowDown", radio)).toBe(false);
    box.remove();
  });
});
