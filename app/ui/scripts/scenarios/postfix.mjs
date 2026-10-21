/** Postfix completion, its settings page and Complete Current Statement. */

/** Put the caret on a new line after the `orders()` relation in User.php. */
async function newLineInUser(page) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "return $this->hasMany(Order::class);" });
  await line.click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
}

/** @param {import("playwright-core").Page} page */
async function openPostfixSettings(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  const node = page.locator('.set-tree-row[data-node="editor.postfix"]');
  if (!(await node.isVisible())) await page.click('.set-tree-row[data-node="editor.general"] .set-tree-chevron');
  await node.click();
  await page.waitForSelector('.postfix-template[data-template="php.foreach"]');
  await page.click('.postfix-template[data-template="php.foreach"]');
  // A cleared template shows the pending change.
  await page.uncheck('.postfix-template[data-template="php.echo"] input');
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "postfix-completion": async (page) => {
    await newLineInUser(page);
    await page.keyboard.type("$this->orders.fo", { delay: 30 });
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(200);
  },
  "postfix-expanded": async (page) => {
    await newLineInUser(page);
    await page.keyboard.type("$this->orders.foreach", { delay: 20 });
    await page.keyboard.press("Escape");
    await page.keyboard.press("Tab");
    await page.waitForTimeout(300);
  },
  "complete-statement": async (page) => {
    await newLineInUser(page);
    await page.keyboard.type("if ($this->orders->isEmpty(", { delay: 20 });
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+Shift+Enter");
    await page.waitForTimeout(300);
  },
  "settings-postfix": async (page) => {
    await openPostfixSettings(page);
  },
  "settings-postfix-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPostfixSettings(page);
  },
};
