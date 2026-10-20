/** Project view context menu on a file, and the New File dialog. */

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "file-menu": async (page) => {
    await page.click('.tree-row[data-path="app/Models/Order.php"]', { button: "right" });
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu-item:has-text("New")');
    await page.waitForSelector(".context-menu .context-menu");
  },
  "file-menu-new-file": async (page) => {
    await page.click('.tree-row[data-path="app/Models"]', { button: "right" });
    await page.click('.context-menu-item:has-text("New")');
    await page.click('.context-menu .context-menu .context-menu-item:has-text("File")');
    await page.waitForSelector(".dialog input");
    await page.fill(".dialog input", "Invoice.php");
  },
};
