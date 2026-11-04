/**
 * The editor's Find/Replace bar (Ctrl+F / Ctrl+R): collapsed Find, expanded Replace, no
 * matches, a regex error, the light and a High Contrast theme, and Find in Files over the bar.
 */

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

/** Put the caret on a word of User.php and open the bar. */
async function open(page, keys = "Control+KeyF") {
  await page.click('.cm-line:has-text("class User")');
  await page.keyboard.press("End");
  await page.keyboard.press("Control+ArrowLeft");
  await page.keyboard.press(keys);
  await page.waitForSelector(".cm-find-bar textarea");
  await page.waitForTimeout(150);
}

async function typeQuery(page, text) {
  await page.fill(".cm-find-bar textarea", text);
  await page.waitForTimeout(150);
}

async function replaceMode(page) {
  await open(page, "Control+KeyR");
  await typeQuery(page, "name");
  await page.fill(".cm-find-bar .fb-row:nth-child(2) textarea", "title");
  await page.click('.cm-find-bar button[aria-label="Preserve case"]');
  await page.locator(".cm-find-bar .fb-row:first-child textarea").focus();
  await page.waitForTimeout(150);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "find-bar-find": async (page) => {
    await open(page);
    await page.keyboard.press("Enter");
    await page.waitForTimeout(150);
  },
  "find-bar-replace": replaceMode,
  "find-bar-no-matches": async (page) => {
    await open(page);
    await typeQuery(page, "doesNotExist");
  },
  "find-bar-regex-error": async (page) => {
    await open(page);
    await page.click('.cm-find-bar button[aria-label="Regex"]');
    await typeQuery(page, "function (");
  },
  "find-bar-history": async (page) => {
    await setting(page, "editor.find.recentFind", ["fillable", "protected $hidden", "User"]);
    await open(page);
    await page.keyboard.press("Alt+ArrowDown");
    await page.waitForSelector(".fb-history");
  },
  "find-bar-light": async (page) => {
    await setting(page, "appearance.theme", "light");
    await page.waitForTimeout(300);
    await replaceMode(page);
  },
  "find-bar-high-contrast": async (page) => {
    await setting(page, "appearance.theme", "high-contrast");
    await page.waitForTimeout(300);
    await replaceMode(page);
  },
  "find-bar-under-find-in-files": async (page) => {
    await open(page);
    await page.keyboard.press("Control+Shift+KeyF");
    await page.waitForSelector(".find-popup");
    await page.waitForTimeout(400);
  },
};
