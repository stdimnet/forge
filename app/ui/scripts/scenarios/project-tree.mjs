/** Project tree with PHPStorm's behavior: single click selects, double click expands folders. */

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "project-tree-double-click": async (page) => {
    // Single click opens by default; this scenario shows the double click mode, so turn it off first.
    await page.evaluate(() =>
      localStorage.setItem("forge.mock.settings.user", JSON.stringify({ "projectView.openFilesWithSingleClick": false })),
    );
    await page.reload();
    await page.waitForSelector(".tree-row");
    await page.click('.tree-row[data-path="database"]');
    await page.dblclick('.tree-row[data-path="resources"]');
    await page.waitForSelector('.tree-row[data-path^="resources/"]');
    await page.click('.tree-row[data-path="routes"]');
    await page.focus(".project-tree");
  },
};
