/** Project tree with PHPStorm's behavior: single click selects, double click expands folders. */

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "project-tree-double-click": async (page) => {
    await page.waitForSelector(".tree-row");
    await page.click('.tree-row[data-path="database"]');
    if (await page.$('.tree-row[data-path^="database/"]')) throw new Error("a single click expanded the folder");
    await page.dblclick('.tree-row[data-path="resources"]');
    await page.waitForSelector('.tree-row[data-path^="resources/"]');
    if (await page.evaluate(() => window.getSelection()?.toString())) throw new Error("the double click selected text");
    await page.click('.tree-row[data-path="routes"]');
    await page.focus(".project-tree");
  },
};
