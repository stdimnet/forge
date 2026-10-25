/**
 * Project structure and Project view options: Mark Directory as › Excluded, excluded folders
 * in the tree and in Settings | Directories, File Nesting, Show Members and the Options menu.
 */

/** @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @param {import("playwright-core").Page} page @param {string} label */
async function menuItem(page, label) {
  return page.locator(".context-menu-label", { hasText: label }).last();
}

/** Expand `storage` and right-click `storage/logs`. @param {import("playwright-core").Page} page */
async function logsMenu(page) {
  await quiet(page);
  const logs = page.locator('.tree-row[data-path="storage/logs"]');
  if (!(await logs.isVisible().catch(() => false))) await page.locator('.tree-row[data-path="storage"]').click();
  await logs.click({ button: "right" });
  await (await menuItem(page, "Mark Directory as")).hover();
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page */
async function exclude(page) {
  await logsMenu(page);
  await (await menuItem(page, "Excluded")).click();
  await page.waitForSelector('.tree-row.excluded[data-path="storage/logs"]');
  await page.locator('.tree-row[data-path="storage/logs"]').click();
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page @param {string} group @param {string} label */
async function option(page, group, label) {
  await page.click('[data-tool-window-panel="project"] [aria-label="Options"]');
  await (await menuItem(page, group)).hover();
  await page.waitForTimeout(200);
  await (await menuItem(page, label)).click();
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "project-view-mark-excluded": logsMenu,
  "project-view-excluded": async (page) => {
    await exclude(page);
    await page.mouse.move(700, 500);
  },
  "project-view-directories-settings": async (page) => {
    await exclude(page);
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="directories"]');
    await page.waitForSelector('.php-dirs-list[data-kind="excluded"] .php-dirs-row');
    await page.locator(".structure-exclude-files input").fill("*.min.js;tmp");
    await page.locator('.php-dirs-list[data-kind="excluded"]').scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
  },
  "project-view-file-nesting": async (page) => {
    await quiet(page);
    await page.locator('.tree-row[data-path="composer.json"] .tree-chevron').click();
    await page.waitForSelector('.tree-row[data-path="composer.lock"]');
    await option(page, "Appearance", "File Nesting");
    await page.waitForSelector(".file-nesting-dialog .nesting-row");
  },
  "project-view-show-members": async (page) => {
    await quiet(page);
    await option(page, "Appearance", "Show Members");
    await page.locator('.tree-row[data-path="app/Models/User.php"] .tree-chevron').click();
    await page.waitForSelector(".project-member-row");
    await page.locator(".project-member-row", { hasText: "User" }).first().locator(".tree-chevron").click().catch(() => undefined);
    await page.waitForTimeout(400);
  },
  "project-view-options": async (page) => {
    await quiet(page);
    await page.click('[data-tool-window-panel="project"] [aria-label="Options"]');
    await (await menuItem(page, "Appearance")).hover();
    await page.waitForTimeout(400);
  },
};
