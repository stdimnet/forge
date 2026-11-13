import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import "@/mock/workspaceMock";
import "@/mock/gitMock";
import "@/mock/gitOpsMock";
import { BranchesPopup, resetBranchFolders } from "./BranchesPopup";
import { opsApi } from "./opsApi";
import { gitApi } from "./api";
import { GROUP_BY_DIRECTORY } from "./branchTree";
import { setLocalSetting } from "@/state/settings";

let dispose: (() => void) | undefined;
beforeEach(() => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <BranchesPopup />, host);
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  setLocalSetting(GROUP_BY_DIRECTORY, true);
  resetBranchFolders();
});

/** The row of a branch in Local or Remote (Recent lists some of them again). */
const popupRow = (branch: string) => document.querySelector<HTMLElement>(`.branch-row[data-branch="${branch}"]:not([data-section="recent"])`);
const folder = (path: string) => document.querySelector<HTMLElement>(`.branch-folder[data-folder="${path}"]`);
const loaded = () => vi.waitFor(() => expect(popupRow("feature/order-totals")).not.toBeNull());

/** Open the folders above a branch (`origin/release/1.1`: `origin/release`), like a user clicking them. */
async function reveal(branch: string): Promise<HTMLElement> {
  await vi.waitFor(() => expect(document.querySelector(".branch-row, .branch-folder")).not.toBeNull());
  const parts = branch.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = folder(parts.slice(0, i).join("/"));
    if (dir?.getAttribute("aria-expanded") === "false") dir.click();
  }
  await vi.waitFor(() => expect(popupRow(branch)).not.toBeNull());
  return popupRow(branch)!;
}

/** The submenu entries of a branch row. */
async function submenu(branch: string): Promise<HTMLElement[]> {
  const row = await reveal(branch);
  row.dispatchEvent(new MouseEvent("mouseenter", { bubbles: false }));
  row.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
  await vi.waitFor(() => expect(document.querySelector(".branch-submenu")).not.toBeNull());
  return [...document.querySelectorAll<HTMLElement>(".branch-submenu .popup-row")];
}

const labels = (rows: HTMLElement[]) => rows.map((r) => r.textContent?.trim());
const row = (rows: HTMLElement[], label: string) => rows.find((r) => r.textContent?.trim() === label)!;

describe("Git Branches popup: branch submenu", () => {
  it("offers Update for a local branch that is not checked out and tracks a branch", async () => {
    const update = vi.spyOn(opsApi, "updateBranch").mockResolvedValue({ output: "Already up to date." });
    const rows = await submenu("main");
    expect(labels(rows)).toContain("Update");
    expect(labels(rows).some((l) => l?.startsWith("Pull into"))).toBe(false);
    row(rows, "Update").click();
    await vi.waitFor(() => expect(update).toHaveBeenCalledWith("main"));
  });

  it("offers no Update for a branch without a tracked branch", async () => {
    const rows = await submenu("bugfix/login-redirect");
    expect(labels(rows)).toContain("Checkout");
    expect(labels(rows)).not.toContain("Update");
  });

  it("offers Pull into the current branch for a remote branch, using rebase or merge", async () => {
    const pull = vi.spyOn(opsApi, "pullInto").mockResolvedValue({ output: "Already up to date." });
    const rows = await submenu("origin/main");
    const names = labels(rows);
    const rebase = "Pull into 'feature/order-totals' Using Rebase";
    const merge = "Pull into 'feature/order-totals' Using Merge";
    expect(names).toContain(rebase);
    expect(names).toContain(merge);
    // Like PHPStorm: after Rebase/Merge, rebase first; a remote branch has no Update or Delete.
    expect(names.indexOf(rebase)).toBeLessThan(names.indexOf(merge));
    expect(names).not.toContain("Update");
    expect(names).not.toContain("Delete");
    row(rows, merge).click();
    await vi.waitFor(() => expect(pull).toHaveBeenCalledWith("origin/main", false));
  });

  it("pulls using rebase", async () => {
    const pull = vi.spyOn(opsApi, "pullInto").mockResolvedValue({ output: "" });
    const rows = await submenu("origin/release/1.1");
    row(rows, "Pull into 'feature/order-totals' Using Rebase").click();
    await vi.waitFor(() => expect(pull).toHaveBeenCalledWith("origin/release/1.1", true));
  });
});

const input = () => document.querySelector<HTMLInputElement>(".branches-popup input")!;
const type = (text: string) => {
  input().value = text;
  input().dispatchEvent(new InputEvent("input", { bubbles: true }));
};
const press = (key: string) => input().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
const activeRow = () => document.getElementById(input().getAttribute("aria-activedescendant") ?? "");
/** The Local and Remote rows as an outline: `> feature` closed, `v feature` open, branches by label. */
const outline = (section: "local" | "remote") =>
  [...document.querySelectorAll<HTMLElement>('.popup-scroll .branch-folder, .popup-scroll .branch-row:not([data-section="recent"])')]
    .filter((r) => (r.dataset.folder ?? r.dataset.branch ?? "").startsWith("origin") === (section === "remote"))
    .map((r) => {
      const label = r.querySelector(".popup-label")!.textContent;
      const mark = r.classList.contains("branch-folder") ? (r.getAttribute("aria-expanded") === "true" ? "v " : "> ") : "";
      return `${"  ".repeat(Number(r.getAttribute("aria-level")) - 1)}${mark}${label}`;
    });

describe("Git Branches popup: folders", () => {
  it("groups local and remote branches by prefix, remote ones below their remote", async () => {
    await loaded();
    // Folders first; the folder of the current branch is open.
    expect(outline("local")).toEqual([
      "> bugfix",
      "v feature",
      "  > ui",
      "  order-totals",
      "  checkout-flow",
      "main",
    ]);
    expect(outline("remote")).toEqual(["v origin", "  > bugfix", "  > feature", "  > release", "  main"]);
    // Recent stays a flat list of full names.
    const recent = [...document.querySelectorAll<HTMLElement>('.branch-row[data-section="recent"]')];
    expect(recent.map((r) => [r.querySelector(".popup-label")!.textContent, r.getAttribute("aria-level")])).toEqual([
      ["main", "1"],
      ["feature/checkout-flow", "1"],
      ["bugfix/login-redirect", "1"],
    ]);
    const current = popupRow("feature/order-totals")!;
    expect(current.getAttribute("aria-level")).toBe("2");
    expect(current.getAttribute("aria-current")).toBe("true");
    expect(document.querySelector(".popup-scroll")!.getAttribute("role")).toBe("tree");
  });

  it("opens and closes a folder with the mouse, nested prefixes nest", async () => {
    await loaded();
    folder("origin/feature")!.click();
    await vi.waitFor(() => expect(folder("origin/feature/ui")).not.toBeNull());
    folder("origin/feature/ui")!.click();
    await vi.waitFor(() => expect(popupRow("origin/feature/ui/dark-mode")).not.toBeNull());
    expect(outline("remote")).toEqual(["v origin", "  > bugfix", "  v feature", "    v ui", "      dark-mode", "    checkout-flow", "    order-totals", "  > release", "  main"]);
    expect(popupRow("origin/feature/ui/dark-mode")!.getAttribute("aria-level")).toBe("4");
    folder("origin/feature")!.click();
    await vi.waitFor(() => expect(popupRow("origin/feature/ui/dark-mode")).toBeNull());
    expect(folder("origin/feature")!.getAttribute("aria-expanded")).toBe("false");
  });

  it("keeps the folders' state when the popup is opened again", async () => {
    await loaded();
    folder("bugfix")!.click();
    await vi.waitFor(() => expect(popupRow("bugfix/login-redirect")).not.toBeNull());
    dispose?.();
    document.body.innerHTML = "";
    const host = document.createElement("div");
    document.body.append(host);
    dispose = render(() => <BranchesPopup />, host);
    await loaded();
    expect(folder("bugfix")!.getAttribute("aria-expanded")).toBe("true");
    expect(popupRow("bugfix/login-redirect")).not.toBeNull();
  });

  it("opens and closes folders with the keyboard", async () => {
    await loaded();
    const closed = folder("bugfix")!;
    expect(closed.getAttribute("aria-expanded")).toBe("false");
    // Down from the search field walks the rows; stop on the folder.
    for (let i = 0; i < 40 && activeRow() !== closed; i++) press("ArrowDown");
    expect(activeRow()).toBe(closed);
    expect(closed.getAttribute("aria-selected")).toBe("true");
    press("ArrowRight");
    await vi.waitFor(() => expect(folder("bugfix")!.getAttribute("aria-expanded")).toBe("true"));
    expect(popupRow("bugfix/login-redirect")).not.toBeNull();
    // Right again goes into the folder, Left back to it, Left again closes it.
    press("ArrowRight");
    expect(activeRow()).toBe(popupRow("bugfix/login-redirect"));
    press("ArrowLeft");
    expect(activeRow()).toBe(folder("bugfix"));
    press("ArrowLeft");
    await vi.waitFor(() => expect(folder("bugfix")!.getAttribute("aria-expanded")).toBe("false"));
    expect(popupRow("bugfix/login-redirect")).toBeNull();
    // Enter toggles a folder too.
    press("Enter");
    await vi.waitFor(() => expect(folder("bugfix")!.getAttribute("aria-expanded")).toBe("true"));
    press("Enter");
    await vi.waitFor(() => expect(folder("bugfix")!.getAttribute("aria-expanded")).toBe("false"));
  });

  it("checks out the branch under the keyboard selection with Enter", async () => {
    const checkout = vi.spyOn(gitApi, "checkout").mockResolvedValue(undefined);
    await loaded();
    type("cart-round");
    await vi.waitFor(() => expect(popupRow("origin/bugfix/cart-rounding")).not.toBeNull());
    expect(activeRow()).toBe(popupRow("origin/bugfix/cart-rounding"));
    press("Enter");
    await vi.waitFor(() => expect(checkout).toHaveBeenCalledWith("origin/bugfix/cart-rounding", true));
  });

  it("finds branches in closed folders and shows them in their open folders", async () => {
    await loaded();
    expect(folder("origin/release")!.getAttribute("aria-expanded")).toBe("false");
    type("1.1");
    await vi.waitFor(() => expect(popupRow("origin/release/1.1")).not.toBeNull());
    expect(outline("remote")).toEqual(["v origin", "  v release", "    1.1"]);
    expect(outline("local")).toEqual([]);
    // The actions of the reworked popup work on a branch inside a folder.
    const pull = vi.spyOn(opsApi, "pullInto").mockResolvedValue({ output: "" });
    const rows = await submenu("origin/release/1.1");
    row(rows, "Pull into 'feature/order-totals' Using Merge").click();
    await vi.waitFor(() => expect(pull).toHaveBeenCalledWith("origin/release/1.1", false));
    // Without the search the folder is closed again.
    type("");
    await vi.waitFor(() => expect(popupRow("origin/release/1.1")).toBeNull());
    expect(folder("origin/release")!.getAttribute("aria-expanded")).toBe("false");
  });

  it("highlights what the search found, in branch and in folder names", async () => {
    await loaded();
    type("ui");
    // The folder `ui` matches by its own name: shown under its open parents, itself closed.
    await vi.waitFor(() => expect(folder("feature/ui")).not.toBeNull());
    expect(outline("local")).toEqual(["v feature", "  > ui"]);
    expect(outline("remote")).toEqual(["v origin", "  v feature", "    > ui"]);
    expect(folder("feature/ui")!.querySelector("mark.branch-hl")!.textContent).toBe("ui");
    expect(folder("feature")!.querySelector("mark")).toBeNull();
    // Its branches stay reachable: open it.
    folder("feature/ui")!.click();
    await vi.waitFor(() => expect(popupRow("feature/ui/dark-mode")).not.toBeNull());
    expect(popupRow("feature/ui/dark-mode")!.querySelector(".popup-label")!.textContent).toBe("dark-mode");
    folder("feature/ui")!.click();

    type("TOTAL");
    await vi.waitFor(() => expect(popupRow("feature/order-totals")!.querySelector("mark.branch-hl")).not.toBeNull());
    const label = popupRow("feature/order-totals")!.querySelector(".popup-label")!;
    expect(label.textContent).toBe("order-totals");
    expect(label.querySelector("mark")!.textContent).toBe("total");
    type("");
    await vi.waitFor(() => expect(document.querySelector("mark.branch-hl")).toBeNull());
  });

  it("leaves Left and Right to the caret while typing", async () => {
    await loaded();
    type("feature");
    await vi.waitFor(() => expect(activeRow()).not.toBeNull());
    const before = activeRow();
    const left = new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true, cancelable: true });
    input().dispatchEvent(left);
    expect(left.defaultPrevented).toBe(false);
    expect(activeRow()).toBe(before);
  });

  it("shows flat lists when Group By Directory is switched off in the settings menu", async () => {
    await loaded();
    const gear = document.querySelector<HTMLElement>('.branches-popup [aria-haspopup="menu"]')!;
    gear.click();
    const item = await vi.waitFor(() => {
      const el = document.querySelector<HTMLElement>(".branch-settings-menu [role=menuitemcheckbox]");
      expect(el).not.toBeNull();
      return el!;
    });
    expect(item.textContent?.trim()).toBe("Group By Directory");
    expect(item.getAttribute("aria-checked")).toBe("true");
    item.click();
    await vi.waitFor(() => expect(document.querySelector(".branch-folder")).toBeNull());
    expect(item.getAttribute("aria-checked")).toBe("false");
    expect(popupRow("bugfix/login-redirect")!.querySelector(".popup-label")!.textContent).toBe("bugfix/login-redirect");
    expect(popupRow("origin/release/1.1")!.getAttribute("aria-level")).toBe("1");
    item.click();
    await vi.waitFor(() => expect(folder("bugfix")).not.toBeNull());
  });
});
