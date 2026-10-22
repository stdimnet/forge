/** Command Line Tool Support: Run Command popup, completion, the console tool window, settings. */

/** @param {import("playwright-core").Page} page */
async function openPopup(page) {
  await page.keyboard.press("Control+Shift+KeyX");
  await page.waitForSelector(".cli-popup .cli-input-field");
  await page.waitForSelector(".cli-alias");
}

/** @param {import("playwright-core").Page} page */
async function type(page, text) {
  await page.keyboard.type(text, { delay: 25 });
  await page.waitForTimeout(250);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "cli-tools-commands": async (page) => {
    await openPopup(page);
    await type(page, "a make:m");
    await page.waitForSelector('.cli-item[data-cli-item="make:model"]');
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(150);
  },
  "cli-tools-options": async (page) => {
    await openPopup(page);
    await type(page, "a make:model Order --");
    await page.waitForSelector('.cli-item[data-cli-item="--migration"]');
    await page.hover('.cli-item[data-cli-item="--migration"]');
    await page.waitForTimeout(150);
  },
  "cli-tools-console": async (page) => {
    await openPopup(page);
    await type(page, "a make:model Order -m");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".cli-window .cli-run-status.ok", { timeout: 8000 });
    await page.click(".cli-window .cli-input-field");
    await type(page, "c up");
    await page.waitForSelector('.cli-item[data-cli-item="update"]');
  },
  "cli-tools-settings": async (page) => {
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("tools.command-line-tools");
    });
    await page.waitForSelector('.cli-tools-editor tr[data-alias="a"]', { timeout: 8000 });
    await page.click('[data-testid="cli-tools-add"]');
    await page.click('.cli-tools-choose-item[data-tool-kind="wp-cli"]');
    await page.waitForSelector('[data-testid="cli-tools-form"]');
    await page.waitForTimeout(200);
  },
  "cli-tools-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPopup(page);
    await type(page, "c require --");
    await page.waitForSelector('.cli-item[data-cli-item="--dev"]');
  },
};
