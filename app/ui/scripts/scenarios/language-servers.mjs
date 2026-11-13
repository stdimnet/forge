/**
 * Language servers for many languages: the Language Servers settings page, installing a
 * server with streamed output, project-wide symbols from several servers in Search
 * Everywhere and indexing progress in the status bar.
 */

/** Freeze the mock TypeScript server's indexing at `percent` (null lets it finish). */
async function tsIndexing(page, percent) {
  await page.evaluate((p) => window.__forgeMockIndexing(p), percent);
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await row.click();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page */
async function openPage(page) {
  await page.evaluate(() => window.__forgeRunCommand("langservers.settings"));
  await page.waitForSelector('[data-testid="language-servers-page"] .ls-tr[data-server]');
  await page.waitForTimeout(150);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "language-servers-settings": async (page) => {
    await openFile(page, "resources/js/app.ts");
    await tsIndexing(page, 45);
    await openPage(page);
    await page.click('.ls-tr[data-server="typescript-language-server"]');
  },
  "language-servers-settings-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPage(page);
    await page.click('.ls-tr[data-server="php-lsp"]');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  },
  "language-servers-details": async (page) => {
    await openPage(page);
    await page.click('.ls-tr[data-server="vscode-json-language-server"]');
    await page.click(".ls-details .ls-link:has-text('Show defaults')");
    await page.fill(".ls-json", '{\n  "json": { "format": { "keepLines": { "enable": true } } }\n}');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  },
  "language-servers-failed": async (page) => {
    await openPage(page);
    await page.fill(".ls-filter input", "xml");
    await page.click('.ls-tr[data-server="lemminx"]');
    await page.waitForSelector('[data-testid="ls-start-error"]');
    await page.click('[data-testid="ls-log-toggle"]');
    await page.waitForSelector('[data-testid="ls-log"]');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  },
  "language-servers-failed-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPage(page);
    await page.fill(".ls-filter input", "xml");
    await page.click('.ls-tr[data-server="lemminx"]');
    await page.waitForSelector('[data-testid="ls-start-error"]');
  },
  "language-servers-install-confirm": async (page) => {
    await openPage(page);
    await page.fill(".ls-filter input", "toml");
    await page.click('.ls-tr[data-server="taplo"] button:has-text("Install")');
    await page.waitForSelector(".ls-install-dialog");
  },
  "language-servers-install-output": async (page) => {
    await openPage(page);
    await page.click('.ls-tr[data-server="docker-compose-langserver"] button:has-text("Install")');
    await page.waitForSelector(".ls-install-dialog");
    await page.click(".ls-install-dialog .btn-primary:has-text('Install')");
    await page.waitForSelector(".ls-log-line:has-text('funding')", { timeout: 10000 });
    await page.waitForSelector(".ls-progress-line .ls-ok");
  },
  "search-everywhere-lsp-symbols": async (page) => {
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Control+Alt+Shift+KeyN");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type("pr");
    await page.waitForSelector(".se-popup .se-row:has-text('products')", { timeout: 5000 });
    await page.waitForTimeout(200);
  },
  "status-bar-indexing": async (page) => {
    await openFile(page, "resources/js/app.ts");
    await tsIndexing(page, 45);
    await page.hover(".lsp-status");
    await page.waitForTimeout(150);
  },
};
