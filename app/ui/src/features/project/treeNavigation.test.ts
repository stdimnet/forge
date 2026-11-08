import { describe, expect, it, vi } from "vitest";
import { listboxKeyDown, navigate, navigateList, optionTabIndex, type VisibleRow } from "./treeNavigation";

const rows: VisibleRow[] = [
  { path: "app", isDir: true, expanded: true, depth: 1 },
  { path: "app/Models", isDir: true, expanded: false, depth: 2 },
  { path: "app/User.php", isDir: false, expanded: false, depth: 2 },
  { path: "routes", isDir: true, expanded: false, depth: 1 },
  { path: "composer.json", isDir: false, expanded: false, depth: 1 },
];

describe("project tree keyboard navigation", () => {
  it("selects the first row when nothing is selected", () => {
    expect(navigate(rows, null, "ArrowDown")).toEqual({ type: "select", path: "app" });
  });

  it("moves up and down and stops at the ends", () => {
    expect(navigate(rows, "app", "ArrowDown")).toEqual({ type: "select", path: "app/Models" });
    expect(navigate(rows, "app/Models", "ArrowUp")).toEqual({ type: "select", path: "app" });
    expect(navigate(rows, "app", "ArrowUp")).toEqual({ type: "none" });
    expect(navigate(rows, "composer.json", "ArrowDown")).toEqual({ type: "none" });
  });

  it("expands closed folders and enters open ones", () => {
    expect(navigate(rows, "app/Models", "ArrowRight")).toEqual({ type: "expand", path: "app/Models" });
    expect(navigate(rows, "app", "ArrowRight")).toEqual({ type: "select", path: "app/Models" });
  });

  it("collapses open folders and jumps to the parent otherwise", () => {
    expect(navigate(rows, "app", "ArrowLeft")).toEqual({ type: "collapse", path: "app" });
    expect(navigate(rows, "app/User.php", "ArrowLeft")).toEqual({ type: "select", path: "app" });
  });

  it("opens files and toggles folders with Enter", () => {
    expect(navigate(rows, "composer.json", "Enter")).toEqual({ type: "open", path: "composer.json" });
    expect(navigate(rows, "routes", "Enter")).toEqual({ type: "expand", path: "routes" });
    expect(navigate(rows, "app", "Enter")).toEqual({ type: "collapse", path: "app" });
  });
});

describe("flat lists (#217)", () => {
  it("moves with the arrows, Home and End", () => {
    const ids = ["a", "b", "c"];
    expect(navigateList(ids, null, "ArrowDown")).toBe("a");
    expect(navigateList(ids, "a", "ArrowDown")).toBe("b");
    expect(navigateList(ids, "c", "ArrowDown")).toBeNull();
    expect(navigateList(ids, "b", "ArrowUp")).toBe("a");
    expect(navigateList(ids, "b", "End")).toBe("c");
    expect(navigateList(ids, "c", "Home")).toBe("a");
    expect(navigateList(ids, "b", "ArrowRight")).toBeNull();
    expect(navigateList(ids, "b", "Enter")).toBeNull();
  });

  it("gives the selected option (or the first) the tab stop", () => {
    expect(optionTabIndex("b", "b", "a")).toBe(0);
    expect(optionTabIndex("a", "b", "a")).toBe(-1);
    expect(optionTabIndex("a", null, "a")).toBe(0);
    expect(optionTabIndex("b", null, "a")).toBe(-1);
  });

  it("selects and focuses the next option, and leaves keys in a row's input alone", () => {
    document.body.innerHTML = `<div role="listbox" id="lb">
      <div role="option" tabindex="0" data-option-id="a"><input id="in" /></div>
      <div role="option" tabindex="-1" data-option-id="b"></div></div>`;
    const lb = document.getElementById("lb")!;
    const select = vi.fn();
    lb.addEventListener("keydown", (e) => listboxKeyDown(e as KeyboardEvent & { currentTarget: HTMLElement }, ["a", "b"], "a", select));
    const opt = lb.querySelector<HTMLElement>('[data-option-id="a"]')!;
    const down = new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true });
    opt.dispatchEvent(down);
    expect(select).toHaveBeenCalledWith("b");
    expect(down.defaultPrevented).toBe(true);
    expect(document.activeElement?.getAttribute("data-option-id")).toBe("b");
    select.mockClear();
    const inInput = new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true });
    document.getElementById("in")!.dispatchEvent(inInput);
    expect(select).not.toHaveBeenCalled();
    expect(inInput.defaultPrevented).toBe(false);
  });
});
