/** The IDE in German (Settings | Appearance & Behavior | System Settings | Language). */

/**
 * Store `ide.language = de` in the browser-mode user settings and start again, so the
 * workbench boots in German like after the restart the language setting offers.
 * @param {import("playwright-core").Page} page
 */
async function german(page) {
  await page.evaluate(() => {
    const key = "forge.mock.settings.user";
    const user = JSON.parse(localStorage.getItem(key) ?? "{}");
    localStorage.setItem(key, JSON.stringify({ ...user, "ide.language": "de" }));
  });
  await page.reload();
  await page.waitForSelector(".cm-content");
}

/** @param {import("playwright-core").Page} page @param {string} label */
async function hover(page, label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".context-menu-label", { hasText: exact }).last().hover();
  await page.waitForTimeout(80);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "i18n-main-window-de": async (page) => {
    await german(page);
    await page.click("[data-main-menu-button]");
    await page.waitForSelector(".context-menu.main-menu-popup");
    await hover(page, "Datei");
    await hover(page, "Neu");
  },
  "i18n-main-menu-code-de": async (page) => {
    await german(page);
    await page.click("[data-main-menu-button]");
    await page.waitForSelector(".context-menu.main-menu-popup");
    await hover(page, "Code");
    await hover(page, "Faltung");
  },
  "i18n-settings-de": async (page) => {
    await german(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="system.language"]');
    await page.waitForTimeout(200);
  },
  "i18n-settings-system-de": async (page) => {
    await german(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="system"]');
    await page.waitForTimeout(200);
  },
  "i18n-find-action-de": async (page) => {
    await german(page);
    await page.keyboard.press("Control+Shift+A");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type("einstell");
    await page.waitForTimeout(400);
  },
  "i18n-find-action-english-query-de": async (page) => {
    // English action names still match, like PHPStorm with a language pack.
    await german(page);
    await page.keyboard.press("Control+Shift+A");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type("reformat");
    await page.waitForTimeout(400);
  },
  "i18n-commit-de": async (page) => {
    await german(page);
    await page.keyboard.press("Control+k");
    await page.waitForSelector(".commit-row");
    await page.click(".commit-options-btn");
    await page.waitForSelector(".commit-options-popup");
  },
  "i18n-welcome-de": Object.assign(
    async (page) => {
      await page.evaluate(() => {
        const key = "forge.mock.settings.user";
        localStorage.setItem(key, JSON.stringify({ ...JSON.parse(localStorage.getItem(key) ?? "{}"), "ide.language": "de" }));
      });
      await page.reload();
      await page.waitForSelector(".welcome-nav");
    },
    { query: "?welcome", waitFor: ".welcome-nav" },
  ),
  "i18n-run-configurations-de": async (page) => {
    await german(page);
    await page.evaluate(() => window.__forgeRunCommand("editRunConfigurations"));
    await page.waitForSelector(".rc-dialog");
    await page.waitForTimeout(200);
  },
};
