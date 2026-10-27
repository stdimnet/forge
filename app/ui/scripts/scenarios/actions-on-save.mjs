/** Settings | Tools | Actions on Save and what an explicit save does with it. */

/** @param {import("playwright-core").Page} page */
async function openPage(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-tree-row");
  const row = page.locator('.set-tree-row[data-node="tools.actions-on-save"]');
  if (!(await row.isVisible())) await page.click('.set-tree-row[data-node="Tools"]');
  await row.click();
  await page.waitForSelector(".aos-row");
}

/** @param {import("playwright-core").Page} page @param {string} key */
const check = (page, key) => page.click(`.aos-row[data-setting="${key}"] input[type="checkbox"]`);

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "actions-on-save-settings": async (page) => {
    await openPage(page);
    await check(page, "actionsOnSave.reformat.enabled");
    await check(page, "actionsOnSave.optimizeImports");
    await check(page, "actionsOnSave.codeCleanup");
    await check(page, "actionsOnSave.pint");
    await page.selectOption('.aos-row[data-setting="actionsOnSave.reformat.enabled"] select', "changed");
    await page.waitForSelector(".aos-muted");
  },
  "actions-on-save-file-types": async (page) => {
    await openPage(page);
    await check(page, "actionsOnSave.reformat.enabled");
    await page.click(".aos-dropdown");
    await page.click('.aos-popup-item:has-text("PHP") >> nth=0');
    await page.waitForSelector(".aos-popup");
  },
  "actions-on-save-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPage(page);
    await check(page, "actionsOnSave.optimizeImports");
    await check(page, "actionsOnSave.phpCsFixer");
  },
  "actions-on-save-run": async (page) => {
    await openPage(page);
    await check(page, "actionsOnSave.optimizeImports");
    await check(page, "actionsOnSave.phpCsFixer");
    await page.click('button:has-text("OK")');
    await page.waitForSelector(".set-dialog", { state: "detached" });
    await page.locator(".cm-content:visible").click({ position: { x: 300, y: 10 } });
    await page.keyboard.press("End");
    await page.keyboard.type(" ");
    await page.keyboard.press("Control+KeyS");
    await page.waitForSelector('text=PHP CS Fixer was skipped');
  },
};
