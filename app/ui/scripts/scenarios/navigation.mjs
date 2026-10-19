/** Screenshots for the PHPStorm keymap, Search Everywhere, Find Action and Recent Files. */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openViaGotoFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForTimeout(250);
  await page.keyboard.press("Enter");
  await page.waitForTimeout(200);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "search-everywhere": async (page) => {
    await page.click(".cm-content");
    await page.keyboard.press("Shift");
    await page.keyboard.press("Shift");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type("order");
    await page.waitForTimeout(300);
    await page.keyboard.press("ArrowDown");
  },
  "find-action": async (page) => {
    await page.keyboard.press("Control+Shift+A");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type("line");
    await page.waitForTimeout(300);
  },
  "goto-file-camel-humps": async (page) => {
    await page.keyboard.press("Control+Shift+N");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type("OrCo");
    await page.waitForTimeout(300);
  },
  "recent-files": async (page) => {
    await openViaGotoFile(page, "OrderController");
    await openViaGotoFile(page, "Checkout");
    await openViaGotoFile(page, "web.php");
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Control+E");
    await page.waitForSelector(".recent-popup");
  },
  "keymap-settings": async (page) => {
    await page.keyboard.press("Control+Alt+S");
    await page.waitForSelector(".keymap-page");
    await page.click(".km-group:has-text('Navigate')");
    await page.waitForTimeout(100);
    await page.click(".km-action[data-action='GotoClass']");
  },
  "keymap-shortcut-conflict": async (page) => {
    await page.keyboard.press("Control+Alt+S");
    await page.waitForSelector(".keymap-page");
    await page.fill(".km-search input", "reformat");
    await page.dblclick(".km-action[data-action='ReformatCode']");
    await page.click(".context-item:has-text('Add Keyboard Shortcut')");
    await page.waitForSelector(".shortcut-dialog");
    await page.keyboard.press("Control+Shift+N");
    await page.waitForTimeout(100);
  },
};
