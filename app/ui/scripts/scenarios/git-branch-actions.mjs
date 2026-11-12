/**
 * The compact Git tool window (tabs in the header).
 */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
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
