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

/** @param {import("playwright-core").Page} page */
async function openTemplateDelimiters(page) {
  await openSettings(page);
  await selectNode(page, "lang.templates");
  await page.locator('[data-setting="templates.delimiters"]').evaluate((el) => el.scrollIntoView({ block: "start" }));
}

/** Open the Twig + LaTeX entry in the edit form. @param {import("playwright-core").Page} page */
async function editTwigLatex(page) {
  await page.dblclick('[data-entry="twig+latex"]');
  await page.waitForSelector(".set-delims-form");
  await page.locator(".set-delims-form").evaluate((el) => el.scrollIntoView({ block: "end" }));
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
  "settings-template-delimiters": async (page) => {
    await openTemplateDelimiters(page);
    await page.click('[data-entry="smarty+latex"]');
  },
  "settings-template-delimiters-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openTemplateDelimiters(page);
    await page.click('[data-entry="smarty+latex"]');
  },
  "settings-template-delimiters-edit": async (page) => {
    await openTemplateDelimiters(page);
    await editTwigLatex(page);
  },
  "settings-template-delimiters-edit-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openTemplateDelimiters(page);
    await editTwigLatex(page);
  },
  "settings-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openSettings(page);
    await page.click('.set-tree-row[data-node="editor.code-style"] .set-tree-chevron');
    await selectNode(page, "editor.code-style.php");
  },
};
