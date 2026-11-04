/** Third-party plugins (the SDK's Word Count example), the Marketplace tab and Settings Sync. */

/** @param {import("playwright-core").Page} page @param {string} id */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/** @param {import("playwright-core").Page} page @param {string} pageId */
async function openSettingsPage(page, pageId) {
  await page.evaluate(async (id) => {
    const { openSettings } = await import("/src/features/settings/dialogState.ts");
    openSettings(id);
  }, pageId);
  await page.waitForSelector(".set-dialog");
}

/** @type {Record<string, ((page: import("playwright-core").Page) => Promise<void>) & { query?: string, waitFor?: string }>} */
const scenarios = {
  "plugin-word-count": async (page) => {
    // The example plugin runs in its sandbox and fills its status item and tool window.
    await page.waitForSelector('[data-plugin-status="example.word-count.status"]');
    await page.evaluate(async () => {
      const { showToolWindow } = await import("/src/state/layout.ts");
      showToolWindow("example.word-count.details");
    });
    await page.waitForTimeout(600);
    await run(page, "example.word-count.show");
    await page.waitForSelector('.balloon:has-text("words")');
  },
  "plugins-marketplace": async (page) => {
    await openSettingsPage(page, "plugins");
    await page.click('[data-testid="plugins-tab-marketplace"]');
    await page.waitForSelector('[data-testid="marketplace-list"] [data-plugin="acme.todo-sync"]');
    await page.click('[data-testid="marketplace-list"] [data-plugin="acme.todo-sync"]');
    await page.waitForSelector('[data-testid="marketplace-details"] [data-permission="network"]');
  },
  "plugins-permission-prompt": async (page) => {
    await openSettingsPage(page, "plugins");
    await page.click('[data-testid="plugins-tab-marketplace"]');
    await page.waitForSelector('[data-testid="marketplace-list"] [data-plugin="acme.changelog"]');
    await page.click('[data-testid="marketplace-list"] [data-plugin="acme.changelog"] button');
    await page.waitForSelector('[data-testid="plugin-permission-prompt"]');
  },
  "plugins-installed-third-party": async (page) => {
    await openSettingsPage(page, "plugins");
    await page.click('.plugin-item[data-plugin="example.word-count"]');
    await page.waitForSelector('[data-testid="plugin-uninstall"]');
  },
  "settings-sync": async (page) => {
    await page.evaluate(async () => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting("settingsSync.enabled", true);
      await saveSetting("settingsSync.folder", "~/Dropbox/Forge");
    });
    await openSettingsPage(page, "settingsSync");
    await page.click('[data-testid="settings-sync-status"] button');
    await page.waitForSelector('[data-testid="settings-sync-status"]:has-text("Last sync")');
  },
};

export default scenarios;
