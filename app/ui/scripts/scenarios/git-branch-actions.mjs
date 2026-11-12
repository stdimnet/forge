/**
 * Git Branches popup: Update on a local branch that is not checked out, Pull into the current
 * branch on a remote branch; and the compact Git tool window (tabs in the header).
 */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

async function submenuOf(page, branch) {
  await run(page, "git.branches");
  await page.waitForSelector(".branches-popup .branch-row");
  await page.locator(`.branches-popup .branch-row[data-branch="${branch}"]`).last().hover();
  await page.waitForSelector(".branch-submenu");
  await page.waitForTimeout(200);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "git-branch-update": async (page) => {
    await submenuOf(page, "main");
    await page.waitForSelector(".branch-submenu .popup-label:text-is('Update')");
  },
  "git-pull-into": async (page) => {
    await submenuOf(page, "origin/main");
    await page.waitForSelector(".branch-submenu .popup-row:has-text('Using Merge')");
  },
  "git-tool-window-compact": async (page) => {
    await run(page, "git.log");
    await page.waitForSelector(".log-row");
    await page.waitForTimeout(300);
  },
  "git-tool-window-compact-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await run(page, "git.log");
    await page.waitForSelector(".log-row");
    await page.waitForTimeout(300);
  },
};

export default scenarios;
