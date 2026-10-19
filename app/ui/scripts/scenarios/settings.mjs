/** Settings dialog scenarios. */

/** @param {import("playwright-core").Page} page */
async function openSettings(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.waitForSelector(".set-row, .plugin-item");
}

/** @param {import("playwright-core").Page} page @param {string} id */
async function selectNode(page, id) {
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForTimeout(100);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "settings-appearance": async (page) => {
    await openSettings(page);
    await selectNode(page, "appearance");
    // Show a pending edit so the modified marker and Reset link are visible.
    await page.check('[data-setting="appearance.compactMode"] input[type="checkbox"]');
  },
  "settings-plugins": async (page) => {
    await openSettings(page);
    await selectNode(page, "plugins");
    await page.click('.plugin-item[data-plugin="forge.php"]');
  },
  "settings-search": async (page) => {
    await openSettings(page);
    await page.fill(".set-sidebar .set-search input", "font");
    await page.waitForTimeout(150);
    await selectNode(page, "editor.font");
  },
  "settings-php": async (page) => {
    await openSettings(page);
    await selectNode(page, "lang.php");
  },
  "settings-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openSettings(page);
    await page.click('.set-tree-row[data-node="editor.code-style"] .set-tree-chevron');
    await selectNode(page, "editor.code-style.php");
  },
};
