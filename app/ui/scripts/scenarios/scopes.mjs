/** Scopes and file colors. */

/** @param {import("playwright-core").Page} page */
async function saveScopes(page) {
  await page.evaluate(async () => {
    const { saveSetting } = await import("/src/state/settings.ts");
    await saveSetting("scopes.shared", [
      { name: "Models", pattern: "file:app/Models//*" },
      { name: "Controllers", pattern: "file:app/Http//*&&!file:app/Http/Controllers/Controller.php" },
    ]);
    await saveSetting("scopes.local", [{ name: "Configuration", pattern: "file:config//*||file:*.yaml||file:.env*" }]);
    await saveSetting("fileColors.shared", [{ scope: "Models", color: "Violet" }]);
  });
}

/** @param {import("playwright-core").Page} page @param {string} id */
async function openSettingsPage(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForTimeout(150);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "scopes-settings": async (page) => {
    await saveScopes(page);
    await openSettingsPage(page, "appearance.scopes");
    await page.click('.scp-item[data-scope="Controllers"]');
    await page.waitForSelector(".scp-tree-row");
    await page.dblclick('.scp-tree-row[data-path="app"]');
    await page.dblclick('.scp-tree-row[data-path="app/Http"]');
    await page.dblclick('.scp-tree-row[data-path="app/Http/Controllers"]');
    await page.click('.scp-tree-row[data-path="app/Services"]');
    await page.getByRole("button", { name: "Include Recursively" }).click();
    await page.waitForTimeout(150);
  },
  "file-colors-settings": async (page) => {
    await saveScopes(page);
    await openSettingsPage(page, "appearance.fileColors");
    await page.waitForSelector(".scp-colors-table");
  },
  "file-colors": async (page) => {
    await saveScopes(page);
    await page.click('.tree-row[data-path="tests"]');
    await page.click('.tree-row[data-path="tests/Unit"]');
    await page.click('.tree-row[data-path="tests/Unit/UserTest.php"]');
    await page.click('.tree-row[data-path="app/Models/Order.php"]');
    await page.waitForSelector(".editor-tab.file-colored");
  },
  "file-colors-light": async (page) => {
    await saveScopes(page);
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.click('.tree-row[data-path="tests"]');
    await page.click('.tree-row[data-path="tests/Unit"]');
    await page.click('.tree-row[data-path="tests/Unit/UserTest.php"]');
    await page.waitForSelector(".editor-tab.file-colored");
  },
  "find-in-files-scope": async (page) => {
    await saveScopes(page);
    await page.keyboard.press("Control+Shift+KeyF");
    await page.waitForSelector(".find-popup");
    await page.fill(".find-input", "function");
    await page.click('.find-scope-tab:has-text("Scope")');
    await page.selectOption(".find-scope-select select", "Models");
    await page.waitForSelector(".find-row");
    await page.waitForTimeout(300);
  },
};
