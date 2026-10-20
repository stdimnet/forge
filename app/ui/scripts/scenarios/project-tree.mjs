/** Project tree: single click selects, double click expands folders. */

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "project-tree-double-click": async (page) => {
    await page.click('.tree-row[data-path="database"]');
    await page.dblclick('.tree-row[data-path="resources"]');
    await page.waitForSelector('.tree-row[data-path^="resources/"]');
    await page.click('.tree-row[data-path="routes"]');
    await page.focus(".project-tree");
  },
};
