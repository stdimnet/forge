/** PHPForge screenshots: Generate popup/chooser/result, Structure tool window, index hover. */

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    // Expand folders that are not open yet (their first child is not rendered).
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await row.dblclick();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).dblclick();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page */
async function caretAtLineEnd(page, text) {
  await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first().click();
  await page.keyboard.press("End");
}

/** @param {import("playwright-core").Page} page */
async function generatePopup(page) {
  await openFile(page, "app/Services/Checkout.php");
  await caretAtLineEnd(page, "private float $taxRate;");
  await page.keyboard.press("Alt+Insert");
  await page.waitForSelector(".php-generate-popup");
}

/** @param {import("playwright-core").Page} page */
async function chooser(page) {
  await generatePopup(page);
  await page.locator(".php-popup-item", { hasText: "Getters and Setters" }).click();
  await page.waitForSelector(".php-dialog");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "php-generate-popup": generatePopup,
  "php-generate-chooser": chooser,
  "php-generate-result": async (page) => {
    await chooser(page);
    await page.locator(".php-dialog .php-btn.primary").click();
    await page.waitForSelector(".php-dialog", { state: "detached" });
    await page.waitForTimeout(200);
  },
  "php-structure": async (page) => {
    await page.waitForSelector(".editor-tab.active");
    await page.keyboard.press("Alt+7");
    await page.waitForSelector(".php-structure-row");
    await page.locator(".php-structure-row", { hasText: "totalSpent" }).click();
  },
  "php-structure-inherited": async (page) => {
    await openFile(page, "app/Models/Order.php");
    await page.keyboard.press("Alt+7");
    await page.waitForSelector(".php-structure-row");
    await page.locator('[data-tool-window-panel="structure"] button[title="Show Inherited"]').click();
    await page.waitForSelector(".php-structure-row.inherited").catch(() => {});
  },
  "php-hover": async (page) => {
    await openFile(page, "app/Http/Controllers/OrderController.php");
    const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "public function show" }).first();
    const box = await line.locator("span", { hasText: /^show$/ }).first().boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.waitForSelector(".php-hover", { timeout: 3000 });
  },
};
