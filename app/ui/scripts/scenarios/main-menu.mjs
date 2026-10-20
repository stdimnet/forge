/** Main menu: the hamburger popup (New UI), the menu bar modes, the About dialog and the setting. */

/** @param {import("playwright-core").Page} page */
async function openHamburger(page) {
  await page.click("[data-main-menu-button]");
  await page.waitForSelector(".context-menu.main-menu-popup");
}

/** Hover a row of the innermost open menu. @param {import("playwright-core").Page} page @param {string} label */
async function hover(page, label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".context-menu-label", { hasText: exact }).last().hover();
  await page.waitForTimeout(80);
}

/** @param {import("playwright-core").Page} page @param {string} id */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "main-menu-file": async (page) => {
    await openHamburger(page);
    await hover(page, "File");
    await hover(page, "Recent Projects");
  },
  "main-menu-tool-windows": async (page) => {
    await openHamburger(page);
    await hover(page, "View");
    await hover(page, "Tool Windows");
  },
  "main-menu-tools": async (page) => {
    // Plugin manifests contribute Tools > Laravel and Tools > Docker.
    await openHamburger(page);
    await hover(page, "Tools");
    await hover(page, "Laravel");
  },
  "main-menu-git": async (page) => {
    // Alt+\ opens the menu from the keyboard with File selected; G jumps to Git.
    await page.click(".cm-content .cm-line:nth-child(5)");
    await page.keyboard.press("Alt+Backslash");
    await page.waitForSelector(".context-menu.main-menu-popup");
    await page.keyboard.press("g");
    await page.waitForSelector(".main-menu-popup .context-menu");
  },
  "main-menu-bar": async (page) => {
    await run(page, "ToggleMainMenuInSeparateToolbar");
    await page.waitForSelector(".main-menu-bar [data-main-menu-index]");
    await page.click('.main-menu-bar [data-main-menu-index="2"]');
    await page.waitForSelector(".context-menu.main-menu-dropdown");
    await hover(page, "Appearance");
  },
  "main-menu-merged": async (page) => {
    await page.evaluate(async () => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting("appearance.mainMenu", "merged");
    });
    await page.waitForSelector(".main-toolbar .main-menu-bar.inline [data-main-menu-index]");
    await page.click('.main-toolbar [data-main-menu-index="4"]');
    await page.waitForSelector(".context-menu.main-menu-dropdown");
    await hover(page, "Folding");
  },
  "main-menu-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openHamburger(page);
    await hover(page, "Edit");
    await hover(page, "Find");
  },
  "main-menu-about": async (page) => {
    await openHamburger(page);
    await hover(page, "Help");
    await page.locator(".context-menu-label", { hasText: /^About$/ }).last().click();
    await page.waitForSelector(".about-dialog");
  },
  "main-menu-settings": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="appearance"]');
    await page.waitForSelector('[data-setting="appearance.mainMenu"]');
    await page.locator('[data-setting="appearance.mainMenu"]').scrollIntoViewIfNeeded();
  },
};
