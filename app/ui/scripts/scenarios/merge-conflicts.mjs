/**
 * Merge conflict resolver: merging feature/checkout-flow stops with conflicts (Conflicts
 * dialog), the three-way merge window (dark and light, before and after applying the
 * non-conflicting changes and Resolve Simple Conflicts), the file deleted on one side, and the
 * conflicted files in the Commit window, Project view and editor banner.
 */

/** Merge feature/checkout-flow through the branches popup. @param {import("playwright-core").Page} page */
async function mergeBranch(page) {
  await page.click('.main-toolbar [title="Git Branches"]');
  await page.waitForSelector(".branch-row");
  await page.locator(".branch-row", { hasText: "feature/checkout-flow" }).first().hover();
  await page.waitForSelector(".branch-submenu");
  await page.locator(".branch-submenu >> text=/^Merge 'feature\\/checkout-flow' into/").click();
  await page.waitForSelector(".conflicts-file");
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page */
async function openMergeWindow(page) {
  await mergeBranch(page);
  await page.dblclick('.conflicts-file[data-path="app/Services/PriceCalculator.php"]');
  await page.waitForSelector(".merge-window .merge-pane-result .cm-content");
  await page.waitForSelector(".merge-band");
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function lightTheme(page) {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await page.waitForTimeout(200);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "merge-conflicts-dialog": async (page) => {
    await mergeBranch(page);
  },
  "merge-conflicts-dialog-grouped": async (page) => {
    await mergeBranch(page);
    await page.click(".conflicts-group input");
    await lightTheme(page);
  },
  "merge-window": async (page) => {
    await openMergeWindow(page);
  },
  "merge-window-resolved": async (page) => {
    await openMergeWindow(page);
    await page.click('.merge-window [title="Apply All Non-Conflicting Changes"]');
    await page.click('.merge-window [title="Resolve Simple Conflicts"]');
    // F7 (Next Difference, through the keymap) to the remaining conflicts; accept the left
    // side of the first one.
    await page.keyboard.press("F7");
    await page.waitForTimeout(300);
    await page.locator(".merge-divider").first().locator(".merge-chunk-btn.apply").first().click();
    await page.keyboard.press("F7");
    await page.waitForTimeout(300);
  },
  "merge-window-light": async (page) => {
    await openMergeWindow(page);
    await lightTheme(page);
  },
  "merge-conflicts-deleted": async (page) => {
    await mergeBranch(page);
    await page.dblclick('.conflicts-file[data-path="app/Reports/SalesReport.php"]');
    await page.waitForSelector("text=Accept Deletion");
    await page.waitForTimeout(200);
  },
  "merge-conflicts-commit": async (page) => {
    await page.waitForSelector(".balloon, .notification-balloon, [role=status]");
    await page.keyboard.press("Control+k");
    await page.waitForSelector(".commit-conflicts");
    await page.dblclick('.tree-row[data-path="app"]').catch(() => {});
    await page.waitForTimeout(200);
    await page.keyboard.press("Control+Shift+N");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type("PriceCalculator");
    await page.waitForFunction(() => document.querySelector(".se-row.selected")?.textContent?.includes("PriceCalculator"), null, { timeout: 8000 });
    await page.keyboard.press("Enter");
    await page.waitForSelector(".merge-banner:not([hidden])");
    await page.waitForTimeout(400);
  },
};
scenarios["merge-conflicts-commit"].query = "?conflicts";

export default scenarios;
