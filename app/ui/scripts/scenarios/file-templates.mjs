/** File and code templates: New ▸ menus, the New PHP Class dialog and the settings page. */

/** @param {import("playwright-core").Page} page */
async function expandApp(page) {
  await page.dblclick('.tree-row[data-path="app"]');
  await page.waitForSelector('.tree-row[data-path="app/Models"]');
}

/** Hover a row of the innermost open menu. @param {import("playwright-core").Page} page @param {string} label */
async function hover(page, label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".context-menu-label", { hasText: exact }).last().hover();
  await page.waitForTimeout(80);
}

/** @param {import("playwright-core").Page} page @param {string} label */
async function pick(page, label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".context-menu-label", { hasText: exact }).last().click();
}

/** @param {import("playwright-core").Page} page */
async function openClassDialog(page) {
  await expandApp(page);
  await page.click('.tree-row[data-path="app/Models"]', { button: "right" });
  await page.waitForSelector(".context-menu");
  await hover(page, "New");
  await hover(page, "PHP");
  await pick(page, "Class");
  await page.waitForSelector(".ft-dialog");
  await page.waitForFunction(() => document.querySelector("#ft-namespace")?.value === "App\\Models");
}

/** @param {import("playwright-core").Page} page @param {string} id */
async function openSettingsPage(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForSelector(".ft-page .ft-item");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "file-templates-new-menu": async (page) => {
    await expandApp(page);
    await page.click('.tree-row[data-path="app/Models"]', { button: "right" });
    await page.waitForSelector(".context-menu");
    await hover(page, "New");
    await hover(page, "PHP");
  },
  "file-templates-php-class-dialog": async (page) => {
    await openClassDialog(page);
    await page.fill("#ft-name", "Invoice");
    await page.fill("#ft-extends", "\\Illuminate\\Database\\Eloquent\\Model");
    await page.fill("#ft-implements", "\\JsonSerializable");
  },
  "file-templates-php-class-created": async (page) => {
    await openClassDialog(page);
    await page.fill("#ft-name", "Invoice");
    await page.fill("#ft-extends", "\\Illuminate\\Database\\Eloquent\\Model");
    await page.click(".ft-dialog .btn-primary");
    await page.waitForSelector('.tree-row[data-path="app/Models/Invoice.php"]');
    await page.waitForFunction(() =>
      [...document.querySelectorAll(".cm-content")].some((el) => el.textContent?.includes("class Invoice extends Model")),
    );
  },
  "file-templates-html-dialog-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await expandApp(page);
    await page.click('.tree-row[data-path="app/Models"]', { button: "right" });
    await page.waitForSelector(".context-menu");
    await hover(page, "New");
    await hover(page, "Web");
    await pick(page, "HTML File");
    await page.waitForSelector(".ft-dialog");
    await page.fill("#ft-name", "invoice");
  },
  "file-templates-enum-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openClassDialog(page);
    await page.fill("#ft-name", "InvoiceStatus");
    await page.selectOption("#ft-template", "PHP Enum");
    await page.fill("#ft-var-BACKED_TYPE", "string");
  },
  "file-templates-new-popup": async (page) => {
    await expandApp(page);
    await page.click('.tree-row[data-path="app/Models"]');
    await page.keyboard.press("Alt+Insert");
    await page.waitForSelector(".context-menu");
  },
  "file-templates-main-menu": async (page) => {
    await page.click("[data-main-menu-button]");
    await page.waitForSelector(".context-menu.main-menu-popup");
    await hover(page, "File");
    await hover(page, "New");
  },
  "file-templates-settings": async (page) => {
    await openSettingsPage(page, "editor.file-templates");
    await page.click('.ft-item[data-template="PHP Class"]');
  },
  "file-templates-settings-code": async (page) => {
    await openSettingsPage(page, "editor.file-templates");
    await page.click('.ft-tab:has-text("Code")');
    await page.click('.ft-item[data-template="PHP Overridden Method Body"]');
  },
};
