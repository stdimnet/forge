import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import "@/mock/workspaceMock";
import "@/mock/gitMock";
import "@/mock/gitOpsMock";
import { setLocalSetting } from "@/state/settings";
import { GROUP_BY_DIRECTORY } from "./branchTree";
import { GitLogPanel } from "./GitLogPanel";

let dispose: (() => void) | undefined;
beforeEach(() => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <GitLogPanel />, host);
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  setLocalSetting(GROUP_BY_DIRECTORY, true);
});

const tree = () => document.querySelector<HTMLElement>(".log-branches")!;
const rows = () => [...tree().querySelectorAll<HTMLElement>("[role=treeitem]")];
const byLabel = (label: string) => rows().find((r) => r.querySelector(".tree-label")!.textContent === label)!;
/** `> name` closed, `v name` open, indented by level. */
const outline = () =>
  rows().map((r) => {
    const open = r.getAttribute("aria-expanded");
    return `${"  ".repeat(Number(r.getAttribute("aria-level")) - 1)}${open === null ? "" : open === "true" ? "v " : "> "}${r.querySelector(".tree-label")!.textContent}`;
  });
const loaded = () => vi.waitFor(() => expect(tree().querySelector('[data-branch="feature/order-totals"]')).not.toBeNull());
const press = (key: string) => tree().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
const cursor = () => document.getElementById(tree().getAttribute("aria-activedescendant") ?? "");

describe("Git tool window, Log tab: branches tree", () => {
  it("groups branches into folders like the Branches popup", async () => {
    await loaded();
    expect(tree().getAttribute("role")).toBe("tree");
    expect(outline()).toEqual([
      "HEAD (Current Branch)",
      "v Local",
      "  > bugfix",
      "  v feature",
      "    order-totals",
      "    checkout-flow",
      "  main",
      "v Remote",
      "  v origin",
      "    > bugfix",
      "    > feature",
      "    > release",
      "    main",
      "> Tags",
    ]);
    expect(tree().querySelector('[data-branch="feature/order-totals"]')!.getAttribute("aria-current")).toBe("true");
  });

  it("opens folders with the mouse and nests nested prefixes", async () => {
    await loaded();
    byLabel("release").click();
    await vi.waitFor(() => expect(tree().querySelector('[data-branch="origin/release/1.1"]')).not.toBeNull());
    expect(tree().querySelector('[data-branch="origin/release/1.1"]')!.getAttribute("aria-level")).toBe("4");
    // A click on a branch in a folder filters the log by its full name.
    tree().querySelector<HTMLElement>('[data-branch="origin/release/1.1"]')!.click();
    await vi.waitFor(() => expect(document.querySelector(".log-filter b")!.textContent).toBe("origin/release/1.1"));
    byLabel("release").click();
    await vi.waitFor(() => expect(tree().querySelector('[data-branch="origin/release/1.1"]')).toBeNull());
  });

  it("walks and opens the tree with the keyboard", async () => {
    await loaded();
    press("ArrowDown"); // HEAD
    press("ArrowDown"); // Local
    press("ArrowDown"); // bugfix
    expect(cursor()).toBe(byLabel("bugfix"));
    expect(cursor()!.getAttribute("aria-selected")).toBe("true");
    press("ArrowRight");
    await vi.waitFor(() => expect(byLabel("bugfix").getAttribute("aria-expanded")).toBe("true"));
    press("ArrowRight");
    expect(cursor()).toBe(tree().querySelector('[data-branch="bugfix/login-redirect"]'));
    press("Enter");
    await vi.waitFor(() => expect(document.querySelector(".log-filter b")!.textContent).toBe("bugfix/login-redirect"));
    press("ArrowLeft");
    expect(cursor()).toBe(byLabel("bugfix"));
    press("ArrowLeft");
    await vi.waitFor(() => expect(byLabel("bugfix").getAttribute("aria-expanded")).toBe("false"));
    expect(tree().querySelector('[data-branch="bugfix/login-redirect"]')).toBeNull();
  });

  it("lists full names without Group By Directory", async () => {
    await loaded();
    setLocalSetting(GROUP_BY_DIRECTORY, false);
    await vi.waitFor(() => expect(tree().querySelector('[data-branch="origin/release/1.1"]')).not.toBeNull());
    expect(outline().slice(0, 9)).toEqual([
      "HEAD (Current Branch)",
      "v Local",
      "  feature/order-totals",
      "  main",
      "  feature/checkout-flow",
      "  bugfix/login-redirect",
      "v Remote",
      "  v origin",
      "    main",
    ]);
    expect(byLabel("release/1.1").getAttribute("aria-level")).toBe("3");
  });
});
