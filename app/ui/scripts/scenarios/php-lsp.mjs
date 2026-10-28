/**
 * The built-in PHP language server php-lsp: its row on the Language Servers page (built in,
 * running, with its version), the PHP settings page with php-lsp chosen (no Intelephense
 * install offer or licence), and the status bar while it serves the demo project.
 */

/** @param {import("playwright-core").Page} page */
async function openLanguageServers(page) {
  await page.evaluate(() => window.__forgeRunCommand("langservers.settings"));
  await page.waitForSelector('[data-testid="language-servers-page"] .ls-tr[data-server="php-lsp"]');
  await page.waitForTimeout(150);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "php-lsp-language-servers": async (page) => {
    await openLanguageServers(page);
    await page.fill(".ls-filter input", "php");
    await page.click('.ls-tr[data-server="php-lsp"]');
    await page.waitForSelector('[data-testid="ls-details"]');
    await page.waitForTimeout(150);
  },
  "php-lsp-language-servers-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openLanguageServers(page);
    await page.fill(".ls-filter input", "php");
    await page.click('.ls-tr[data-server="php-lsp"]');
    await page.waitForSelector('[data-testid="ls-details"]');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
    await page.waitForTimeout(150);
  },
  "php-lsp-settings": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.click('.set-tree-row[data-node="lang.php"]');
    await page.waitForSelector('.php-radio:has-text("php-lsp (built-in)")');
    await page.waitForSelector('.php-radio:has-text("0.1.0")');
    await page.waitForTimeout(150);
  },
  "php-lsp-status": async (page) => {
    await page.waitForSelector('.lsp-status:has-text("php-lsp")', { timeout: 10000 });
    await page.hover(".lsp-status");
    await page.waitForTimeout(300);
  },
};
