/** Settings | Editor | Code Style: schemes, language pages with presets and the live preview. */

/** @param {import("playwright-core").Page} page @param {string} id */
async function openCodeStyle(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  const row = page.locator(`.set-tree-row[data-node="${id}"]`);
  if (!(await row.count())) await page.click('.set-tree-row[data-node="editor.code-style"] .set-tree-chevron');
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForSelector(".cs-scheme-bar");
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page @param {string} tab */
async function tab(page, tab) {
  await page.click(`.cs-tab[data-tab="${tab}"]`);
  await page.waitForTimeout(400);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "code-style-general": async (page) => {
    await openCodeStyle(page, "editor.code-style");
  },
  "code-style-formatter-markers": async (page) => {
    await openCodeStyle(page, "editor.code-style");
    await tab(page, "formatter");
    await page.click('[data-option="general.formatterTags.enabled"] input');
    await page.waitForTimeout(500);
  },
  "code-style-php-spaces": async (page) => {
    await openCodeStyle(page, "editor.code-style.php");
    await tab(page, "spaces");
  },
  "code-style-php-set-from": async (page) => {
    await openCodeStyle(page, "editor.code-style.php");
    await page.click(".cs-set-from");
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu >> text="Predefined Style"');
    await page.waitForTimeout(300);
  },
  "code-style-php-wordpress": async (page) => {
    await openCodeStyle(page, "editor.code-style.php");
    await page.click(".cs-set-from");
    await page.hover('.context-menu >> text="Predefined Style"');
    await page.click('.context-menu >> text="WordPress"');
    await tab(page, "wrappingAndBraces");
  },
  "code-style-scheme-menu": async (page) => {
    await openCodeStyle(page, "editor.code-style.php");
    await page.click(".cs-gear");
    await page.waitForSelector(".context-menu");
    await page.waitForTimeout(200);
  },
  "code-style-javascript": async (page) => {
    await openCodeStyle(page, "editor.code-style.javascript");
    await page.click(".cs-set-from");
    await page.hover('.context-menu >> text="Predefined Style"');
    await page.click('.context-menu >> text="JavaScript Standard Style"');
    await tab(page, "punctuation");
  },
  "code-style-yaml": async (page) => {
    await openCodeStyle(page, "editor.code-style.yaml");
  },
  "code-style-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openCodeStyle(page, "editor.code-style.php");
    await tab(page, "blankLines");
  },
};
