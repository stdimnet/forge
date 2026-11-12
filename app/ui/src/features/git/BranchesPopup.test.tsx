import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import "@/mock/workspaceMock";
import "@/mock/gitMock";
import "@/mock/gitOpsMock";
import { BranchesPopup } from "./BranchesPopup";
import { opsApi } from "./opsApi";

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
});

/** The submenu entries of a branch row (the last row with that name: Local, not Recent). */
async function submenu(branch: string): Promise<HTMLElement[]> {
  await vi.waitFor(() => expect(document.querySelector(`.branch-row[data-branch="${branch}"]`)).not.toBeNull());
  const rows = document.querySelectorAll<HTMLElement>(`.branch-row[data-branch="${branch}"]`);
  rows[rows.length - 1].dispatchEvent(new MouseEvent("mouseenter", { bubbles: false }));
  rows[rows.length - 1].dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
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
