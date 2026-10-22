/**
 * Git Log actions: the commit context menu (with disabled actions and their reasons), several
 * selected commits, the "Rebasing Commits" dialog with squash and reword, Edit Commit Message
 * and Squash Commits.
 */
const openLog = async (page) => {
  await page.evaluate(() => window.__forgeRunCommand("git.log"));
  await page.waitForSelector(".log-row");
  // Only the current branch, so the rows are the commits that can be rewritten.
  await page.click(".log-branches .tree-row:has-text('HEAD (Current Branch)')");
  await page.waitForTimeout(300);
};
const row = (page, n) => page.locator(".log-list .log-row").nth(n);

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "git-log-pushed-warning": async (page) => {
    await openLog(page);
    await row(page, 1).click({ button: "right" });
    await page.click(".context-menu-item:has-text('Edit Commit Message')");
    await page.waitForSelector(".dialog-buttons .btn-primary:has-text('Continue')");
    await page.waitForTimeout(200);
  },
  "git-log-context-menu": async (page) => {
    await openLog(page);
    await row(page, 1).click({ button: "right" });
    await page.waitForSelector(".context-menu-item:has-text('Interactively Rebase from Here')");
    await page.hover(".context-menu-item:has-text('Undo Commit')");
    await page.waitForTimeout(300);
  },
  "git-log-multiple-commits": async (page) => {
    await openLog(page);
    await row(page, 0).click();
    await row(page, 2).click({ modifiers: ["Shift"] });
    await row(page, 1).click({ button: "right" });
    await page.waitForSelector(".context-menu-item:has-text('Squash Commits')");
    await page.hover(".context-menu-item:has-text('Squash Commits')");
    await page.waitForTimeout(300);
  },
  "git-interactive-rebase": async (page) => {
    await openLog(page);
    await row(page, 2).click({ button: "right" });
    await page.click(".context-menu-item:has-text('Interactively Rebase from Here')");
    // The oldest commit is already pushed to origin/feature/order-totals: confirm the warning.
    await page.click(".dialog-buttons .btn-primary:has-text('Continue')");
    await page.waitForSelector(".rebase-dialog .rebase-row");
    // Squash the middle commit into the oldest one, reword the newest.
    await page.click(".rebase-row >> nth=1");
    await page.click(".rebase-tool:has-text('Squash')");
    await page.click(".rebase-row >> nth=0");
    await page.click(".rebase-tool:has-text('Reword')");
    await page.fill(".rebase-editor textarea", "Add order totals to the checkout\n\nComputed from the order lines, without taxes.");
    await page.click(".rebase-row >> nth=1");
    await page.waitForTimeout(300);
  },
  "git-edit-commit-message": async (page) => {
    await openLog(page);
    await row(page, 0).click({ button: "right" });
    await page.click(".context-menu-item:has-text('Edit Commit Message')");
    await page.waitForSelector(".log-message-dialog textarea");
    await page.waitForTimeout(200);
  },
};

scenarios["git-interactive-rebase-light"] = Object.assign(async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await scenarios["git-interactive-rebase"](page);
});

export default scenarios;
