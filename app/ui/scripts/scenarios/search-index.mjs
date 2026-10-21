/** Classes and Symbols from the search index, and the Find tool window. */

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "search-index-symbols": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("SearchEverywhere"));
    await page.keyboard.type("order");
    await page.waitForTimeout(800);
  },
  "search-index-find-scopes": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("FindInPath"));
    await page.keyboard.type("order");
    await page.click('.find-scope-tab:has-text("Open Files")');
    await page.waitForTimeout(500);
  },
  "search-index-status": async (page) => {
    await page.waitForSelector('[data-testid="index-status"]');
    await page.hover('[data-testid="index-status"]');
  },
  "search-index-find-window": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("FindInPath"));
    await page.keyboard.type("order");
    await page.waitForTimeout(500);
    await page.click('button[title^="Open in Find Window"]');
    await page.waitForSelector(".fw-match");
  },
};
