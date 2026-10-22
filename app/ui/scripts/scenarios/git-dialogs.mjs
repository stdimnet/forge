/**
 * Git menu dialogs and the shelf: Stash, Unstash (with View), Pull, Merge (with options),
 * Rebase, Reset HEAD, Tag, Git Remotes, Shelve Changes, the Stash and Shelf tabs of the Commit
 * tool window, Unshelve, and the branch widget during a rebase ("Rebasing …").
 */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);
const dialog = (page) => page.waitForSelector(".gd[role=dialog]");

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "git-stash-dialog": async (page) => {
    await run(page, "Git.Stash");
    await dialog(page);
    await page.keyboard.type("Half-done invoice PDF layout");
    await page.waitForTimeout(200);
  },
  "git-unstash-dialog": async (page) => {
    await run(page, "Git.Unstash");
    await page.waitForSelector(".gd [data-stash]");
    await page.click(".gd-side-buttons button:has-text('View')");
    await page.waitForSelector(".gd-files .gd-file");
    await page.click(".gd-checks label:has-text('Pop stash') input");
    await page.waitForTimeout(200);
  },
  "git-pull-dialog": async (page) => {
    await run(page, "Git.Pull");
    await dialog(page);
    await page.click(".gd-modify");
    await page.click(".gd-option:has-text('--rebase') input");
    await page.waitForTimeout(200);
  },
  "git-merge-dialog": async (page) => {
    await run(page, "Git.Merge");
    await dialog(page);
    await page.keyboard.type("feature/checkout-flow");
    await page.click(".gd-modify");
    await page.click(".gd-option:has-text('--no-ff') input");
    await page.click(".gd-option:has-text('-m') >> nth=0");
    await page.waitForSelector(".gd-textarea");
    await page.fill(".gd-textarea", "Merge the checkout flow");
    await page.waitForTimeout(200);
  },
  "git-rebase-dialog": async (page) => {
    await run(page, "Git.Rebase");
    await dialog(page);
    await page.keyboard.type("main");
    await page.click(".gd-modify");
    await page.click(".gd-option:has-text('--update-refs') input");
    await page.waitForTimeout(200);
  },
  "git-reset-dialog": async (page) => {
    await run(page, "Git.Reset");
    await dialog(page);
    await page.fill(".gd-commit .gd-input", "HEAD~1");
    await page.click(".gd-commit button");
    await page.waitForSelector(".gd-valid, .gd-invalid");
    await page.click(".gd-radio:has-text('Hard') input");
    await page.waitForTimeout(200);
  },
  "git-tag-dialog": async (page) => {
    await run(page, "Git.Tag");
    await dialog(page);
    await page.keyboard.type("v1.2.0");
    await page.fill(".gd .gd-textarea", "Release 1.2.0: order totals and checkout");
    await page.waitForTimeout(200);
  },
  "git-remotes-dialog": async (page) => {
    await run(page, "Git.Configure.Remotes");
    await page.waitForSelector(".gd-table tbody tr");
    await page.click(".gd-table tbody tr");
    await page.waitForTimeout(200);
  },
  "git-shelve-dialog": async (page) => {
    await run(page, "ChangesView.Shelve");
    await page.waitForSelector(".gd-file-checks .gd-file");
    await page.waitForTimeout(200);
  },
  "git-shelf-tab": async (page) => {
    await run(page, "Vcs.Show.Shelf");
    await page.waitForSelector(".shelf-panel [data-shelf]");
    await page.click(".shelf-panel [data-shelf] .tree-chevron");
    await page.click(".shelf-panel .commit-row >> nth=0");
    await page.waitForTimeout(300);
  },
  "git-unshelve-dialog": async (page) => {
    await run(page, "Vcs.Show.Shelf");
    await page.waitForSelector(".shelf-panel [data-shelf]");
    await page.click(".shelf-panel [data-shelf] .tree-chevron");
    await page.click(".shelf-panel .commit-row >> nth=0");
    await page.keyboard.down("Control");
    await page.click(".shelf-panel .commit-row >> nth=1");
    await page.keyboard.up("Control");
    await run(page, "ShelveChanges.UnshelveWithDialog");
    await page.waitForSelector(".gd-file-checks");
    await page.waitForTimeout(200);
  },
  "git-stash-tab": async (page) => {
    await page.keyboard.press("Alt+0");
    await page.waitForSelector("[data-commit-tab=stash]");
    await page.click("[data-commit-tab=stash]");
    await page.waitForSelector(".stash-panel [data-stash]");
    await page.click(".stash-panel [data-stash] .tree-chevron");
    await page.waitForSelector(".stash-panel .commit-row");
    await page.click(".stash-panel .commit-row >> nth=0");
    await page.waitForTimeout(300);
  },
  "git-rebasing": async (page) => {
    await page.waitForFunction(() => document.querySelector(".main-toolbar")?.textContent?.includes("Rebasing"), null, { timeout: 8000 });
    await page.evaluate(() => document.querySelector(".modal-backdrop")?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    await page.keyboard.press("Escape");
    await page.keyboard.press("Alt+Backslash");
    await page.waitForSelector(".main-menu-popup");
    await page.hover(".main-menu-popup >> text=Git");
    await page.waitForTimeout(600);
  },
};
scenarios["git-shelf-diff"] = async (page) => {
  await scenarios["git-shelf-tab"](page);
  await page.dblclick(".shelf-panel .commit-row >> nth=0");
  await page.waitForSelector(".diff-viewer");
  await page.waitForTimeout(500);
};
scenarios["git-unstash-dialog-light"] = async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await scenarios["git-unstash-dialog"](page);
};
scenarios["git-rebasing"].query = "?rebasing";

export default scenarios;
