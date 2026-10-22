/** Settings | Tools | External Tools, the Edit Tool and Macros dialogs, menus and a tool run. */

/** @param {import("playwright-core").Page} page */
async function openPage(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-tree-row");
  const row = page.locator('.set-tree-row[data-node="tools.external-tools"]');
  if (!(await row.isVisible())) await page.click('.set-tree-row[data-node="Tools"]');
  await row.click();
  await page.waitForSelector(".et-tree-tool");
}

/** Hover a row of the innermost open menu. @param {import("playwright-core").Page} page @param {string} label */
async function hover(page, label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".context-menu-label", { hasText: exact }).last().hover();
  await page.waitForTimeout(80);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "external-tools-settings": async (page) => {
    await openPage(page);
    await page.click('.et-tree-tool[data-tool="PHPStan"]');
  },
  "external-tools-edit": async (page) => {
    await openPage(page);
    await page.dblclick('.et-tree-tool[data-tool="PHPStan"]');
    await page.waitForSelector(".et-dialog");
  },
  "external-tools-macros": async (page) => {
    await page.click(".cm-content");
    await openPage(page);
    await page.dblclick('.et-tree-tool[data-tool="PHPStan"]');
    await page.click('.et-row:has(#et-args) .et-macro-btn');
    await page.waitForSelector(".et-macro");
    await page.click('.et-macro:has-text("$FileDirRelativeToProjectRoot$")');
    await page.waitForSelector('[data-testid="et-macro-preview"]:has-text("app/Models")');
  },
  "external-tools-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPage(page);
    await page.click('.et-toolbar, .set-list-toolbar button[title="Add"]');
    await page.waitForSelector(".et-dialog");
    await page.fill("#et-name", "Pint");
    await page.fill("#et-program", "vendor/bin/pint");
    await page.fill("#et-args", "$FilePath$");
    await page.fill("#et-filters", "$FILE_PATH$:$LINE$ (");
    await page.click('.et-dialog .btn-primary:has-text("OK")');
    await page.waitForSelector(".et-problems");
  },
  "external-tools-menu": async (page) => {
    await page.click("[data-main-menu-button]");
    await page.waitForSelector(".context-menu.main-menu-popup");
    await hover(page, "Tools");
    await hover(page, "External Tools");
  },
  "external-tools-editor-menu": async (page) => {
    await page.click(".cm-content", { button: "right" });
    await page.waitForSelector(".context-menu");
    await hover(page, "Linters");
  },
  "external-tools-run": async (page) => {
    await page.click(".cm-content");
    await page.evaluate(() => window.__forgeRunCommand("Tool_Linters_PHPStan"));
    await page.waitForSelector('.run-tab:has-text("PHPStan")');
    await page.waitForSelector(".run-console a, .run-console .console-link, .run-console [data-link]", { timeout: 5000 }).catch(() => undefined);
    await page.waitForTimeout(800);
  },
};
