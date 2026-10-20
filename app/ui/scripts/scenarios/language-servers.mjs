/**
 * Language servers for many languages: the Language Servers settings page, installing a
 * server with streamed output, the Intelephense licence (Free → Premium), project-wide
 * symbols from several servers in Search Everywhere and indexing progress in the status bar.
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
    if (!(await child.isVisible().catch(() => false))) await row.dblclick();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).dblclick();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page */
async function openPage(page) {
  await page.evaluate(() => window.__forgeRunCommand("langservers.settings"));
  await page.waitForSelector('[data-testid="language-servers-page"] .ls-tr[data-server]');
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page */
async function openPhpPage(page) {
  await page.evaluate(() => window.__forgeRunCommand("settings.open"));
  await page.click('.set-tree-row[data-node="lang.php"]');
  await page.waitForSelector('[data-testid="intelephense-licence"]');
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
    await page.click('.ls-tr[data-server="intelephense"]');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  },
  "language-servers-details": async (page) => {
    await openPage(page);
    await page.click('.ls-tr[data-server="vscode-json-language-server"]');
    await page.click(".ls-details .ls-link:has-text('Show defaults')");
    await page.fill(".ls-json", '{\n  "json": { "format": { "keepLines": { "enable": true } } }\n}');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
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
  "intelephense-licence-free": async (page) => {
    await openPhpPage(page);
  },
  "intelephense-licence-dialog": async (page) => {
    await openPhpPage(page);
    await page.click('[data-testid="intelephense-licence"] button:has-text("Enter Licence Key")');
    await page.waitForSelector(".ls-licence-dialog");
    await page.fill("#ls-licence-key", "00K7M2Q9X4D81234");
  },
  "intelephense-licence-premium": async (page) => {
    await openPhpPage(page);
    await page.click('[data-testid="intelephense-licence"] button:has-text("Enter Licence Key")');
    await page.fill("#ls-licence-key", "00K7M2Q9X4D81234");
    await page.click(".ls-licence-dialog .btn-primary:has-text('Activate')");
    await page.waitForSelector(".ls-tier.premium");
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
