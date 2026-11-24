/** Project view context menu on a file, the New File dialog, and Cut / Copy / Paste of files. */

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
  "file-menu-clipboard": async (page) => {
    // After Copy, Paste is enabled for the folder under the mouse.
    await page.click('.tree-row[data-path="app/Models/Order.php"]', { button: "right" });
    await page.click('.context-menu-item:has-text("Copy"):not(:has-text("…")):not(:has-text("/"))');
    await page.click('.tree-row[data-path="app/Models"]', { button: "right" });
    await page.waitForSelector(".context-menu");
  },
  "project-paste-duplicate": async (page) => {
    // Ctrl+C, Ctrl+V on a file: the Copy dialog asks for the name of the duplicate.
    await page.click('.tree-row[data-path="routes"]');
    await page.keyboard.press("ArrowRight");
    await page.click('.tree-row[data-path="routes/web.php"]');
    await page.keyboard.press("Control+KeyC");
    await page.keyboard.press("Control+KeyV");
    await page.waitForSelector(".dialog input");
  },
};
