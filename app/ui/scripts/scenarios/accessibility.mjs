/**
 * Accessibility: the High Contrast themes (dark and light), the Presentation Assistant with the
 * alternate keymap, keyboard focus rings and the Accessibility settings.
 */

/** Select a theme in the settings, as Settings | Appearance | Theme does. */
async function withTheme(page, theme) {
  await setting(page, "appearance.theme", theme);
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page */
async function setting(page, key, value) {
  await page.evaluate(
    async ([key, value]) => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting(key, value);
    },
    [key, value],
  );
}

/** Project and Commit windows with some selection, for a busy screen. */
async function busy(page) {
  if (!(await page.$(".project-tree"))) await page.keyboard.press("Alt+1");
  await page.waitForSelector('.project-tree .tree-row[data-path="app"]');
  await page.click('.project-tree .tree-row[data-path="app"]');
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "a11y-high-contrast-dark": async (page) => {
    await withTheme(page, "high-contrast");
    await busy(page);
  },
  "a11y-high-contrast-light": async (page) => {
    await withTheme(page, "high-contrast-light");
    await busy(page);
  },
  "a11y-high-contrast-settings": async (page) => {
    await withTheme(page, "high-contrast");
    await page.keyboard.press("Control+Alt+s");
    await page.waitForSelector(".set-tree-row");
    await page.keyboard.type("accessib");
    await page.waitForTimeout(400);
  },
  "a11y-presentation-assistant": async (page) => {
    await setting(page, "appearance.presentationAssistant.enabled", true);
    await page.keyboard.press("Control+Shift+A");
    await page.waitForSelector(".se-popup");
    await page.keyboard.press("Escape");
    await page.waitForTimeout(100);
  },
  "a11y-presentation-assistant-large": async (page) => {
    await setting(page, "appearance.presentationAssistant.enabled", true);
    await setting(page, "appearance.presentationAssistant.size", "large");
    await setting(page, "appearance.presentationAssistant.position", "top-center");
    await page.locator(".editor-slot:not(.hidden) .cm-content").focus();
    await page.keyboard.press("Control+d");
    await page.waitForTimeout(150);
  },
  "a11y-focus-rings": async (page) => {
    // Keyboard only: Ctrl+Shift+F6 walks the areas; Tab moves to a toolbar button.
    await page.locator(".editor-slot:not(.hidden) .cm-content").focus();
    await page.keyboard.press("Control+Shift+F6");
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    await page.waitForTimeout(150);
  },
  "a11y-focus-rings-tree": async (page) => {
    await busy(page);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Alt+1");
    await page.keyboard.press("Alt+1");
    await page.waitForTimeout(200);
    await page.keyboard.press("ArrowDown");
  },
  "a11y-settings-page": async (page) => {
    await page.keyboard.press("Control+Alt+s");
    await page.waitForSelector(".set-tree-row");
    await page.keyboard.type("screen reader");
    await page.waitForTimeout(400);
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
  },
};
