/** Editor context menu (right-click) with a submenu open. */

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "context-menu-editor": async (page) => {
    await page.click(".cm-content .cm-line:nth-child(12)");
    await page.click(".cm-content .cm-line:nth-child(12)", { button: "right" });
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu-item:has-text("Go To")');
    await page.waitForSelector(".context-menu .context-menu");
  },
};
