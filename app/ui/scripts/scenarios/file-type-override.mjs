/**
 * File types by `#!` line (Laravel's `artisan` opens as PHP, also after typing the line) and
 * File | File Properties | Override File Type.
 */

/** @param {import("playwright-core").Page} page */
async function openArtisan(page) {
  await page.dblclick('.tree-row[data-path="artisan"]');
  await page.waitForSelector('.editor-tab.active[data-tab-id="artisan"]');
  await page.waitForSelector('.status-item[data-status="file-type"]', { hasText: "PHP" });
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function fileType(page) {
  return page.locator('.status-item[data-status="file-type"]').innerText();
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "file-type-shebang": async (page) => {
    await openArtisan(page);
    // Live re-detection: remove the `#!` line (Plain text), then type it again (PHP).
    const first = page.locator(".editor-slot:not(.hidden) .cm-line").first();
    await first.click();
    await page.keyboard.press("Home");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Delete");
    await page.waitForSelector('.status-item[data-status="file-type"]', { hasText: "Plain text" });
    await page.keyboard.type("#!/usr/bin/env php");
    await page.waitForSelector('.status-item[data-status="file-type"]', { hasText: "PHP" });
    if ((await fileType(page)) !== "PHP") throw new Error(`artisan is ${await fileType(page)}`);
    await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "require" }).first().click();
    await page.waitForTimeout(400);
  },
  "file-type-override": async (page) => {
    await openArtisan(page);
    await page.evaluate(() => window.__forgeRunCommand("OverrideFileTypeAction"));
    await page.waitForSelector('[data-testid="file-type-override-popup"] .se-row');
    await page.keyboard.type("sh");
    await page.waitForTimeout(500);
  },
  "file-type-override-applied": async (page) => {
    await openArtisan(page);
    await page.evaluate(() => window.__forgeRunCommand("OverrideFileTypeAction"));
    await page.waitForSelector('[data-testid="file-type-override-popup"] .se-row');
    await page.keyboard.type("Shell Script");
    await page.waitForTimeout(400);
    await page.keyboard.press("Enter");
    await page.waitForSelector('.status-item[data-status="file-type"].status-overridden', { hasText: "Shell Script" });
    // Revert goes back to the detected type.
    await page.evaluate(() => window.__forgeRunCommand("RevertFileTypeAction"));
    await page.waitForSelector('.status-item[data-status="file-type"]:not(.status-overridden)', { hasText: "PHP" });
    // Override again for the screenshot, with the tab menu open.
    await page.evaluate(() => window.__forgeRunCommand("OverrideFileTypeAction"));
    await page.waitForSelector('[data-testid="file-type-override-popup"] .se-row');
    await page.keyboard.type("Shell Script");
    await page.waitForTimeout(400);
    await page.keyboard.press("Enter");
    await page.waitForSelector('.status-item[data-status="file-type"].status-overridden');
    await page.click('.editor-tab.active[data-tab-id="artisan"]', { button: "right" });
    await page.waitForTimeout(400);
  },
};
