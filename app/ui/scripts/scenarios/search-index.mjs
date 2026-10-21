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
  "search-index-find-context": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("FindInPath"));
    await page.keyboard.type("hasMany");
    await page.waitForTimeout(500);
    await page.click('button[title^="Open in Find Window"]');
    await page.waitForSelector(".fw-match");
    await page.selectOption(".fw-context-select", "2");
    await page.waitForSelector(".fw-context");
  },
  "search-index-find-sorted": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("FindInPath"));
    await page.keyboard.type("order");
    await page.waitForTimeout(500);
    await page.click('button[title^="Open in Find Window"]');
    await page.waitForSelector(".fw-match");
    await page.selectOption('select[title="Order of the files"]', "matches");
    await page.waitForTimeout(500);
  },
  "search-index-exact-toggle": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("GotoFile"));
    await page.keyboard.type("ordc");
    await page.waitForTimeout(400);
    await page.click('.se-search button[aria-pressed]');
    await page.waitForTimeout(400);
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
