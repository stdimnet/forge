/**
 * Intelephense "click and play": a PHP project without Intelephense offers to install it,
 * the licence is accepted once, Forge downloads Node.js and Intelephense with progress in the
 * status bar and the server runs afterwards. Also the one-click Install buttons on the
 * Language Servers page. Browser mode simulates the downloads (`?intelephense=missing`).
 */

const OFFER = '.balloon:has-text("Intelephense is not installed")';

/** Hold the simulated download at `fraction` (undefined lets it finish). */
async function holdAt(page, fraction) {
  await page.evaluate((f) => {
    window.__forgeMockInstallHold = f ?? undefined;
  }, fraction);
}

/** @param {import("playwright-core").Page} page */
async function acceptLicence(page) {
  await page.locator(`${OFFER} .balloon-action:has-text("Install")`).first().click();
  await page.waitForSelector('[data-testid="intelephense-consent"]');
  await page.click('[data-testid="intelephense-consent"] .btn-primary');
}

/** @param {(page: import("playwright-core").Page) => Promise<void>} fn */
const missing = (fn) => Object.assign(fn, { query: "?intelephense=missing", waitFor: OFFER });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "intelephense-install-offer": missing(async (page) => {
    await page.waitForTimeout(200);
  }),
  "intelephense-licence": missing(async (page) => {
    await page.locator(`${OFFER} .balloon-action:has-text("Install")`).first().click();
    await page.waitForSelector('[data-testid="intelephense-consent"]');
  }),
  "intelephense-install-progress": missing(async (page) => {
    await holdAt(page, 0.42);
    await acceptLicence(page);
    await page.waitForSelector('[data-testid="install-status"]:has-text("Node.js")');
    await page.waitForFunction(() => /\d+%/.test(document.querySelector('[data-testid="install-status"]')?.textContent ?? ""));
    await page.waitForTimeout(500);
  }),
  "intelephense-installed-running": missing(async (page) => {
    await acceptLicence(page);
    await page.waitForSelector('.balloon:has-text("Intelephense installed")', { timeout: 20000 });
    await page.waitForSelector('[data-testid="install-status"]', { state: "detached" });
    // The language server widget reports Intelephense once it finished indexing.
    await page.waitForTimeout(1200);
  }),
  "language-servers-install-buttons": missing(async (page) => {
    await page.locator(`${OFFER} .balloon-close`).click();
    await page.evaluate(() => window.__forgeRunCommand("langservers.settings"));
    await page.waitForSelector('[data-testid="language-servers-page"] .ls-tr[data-server]');
    // One server downloading, one waiting for a click.
    await holdAt(page, 0.6);
    await page.click('[data-testid="install-docker-compose-langserver"]');
    await page.waitForSelector('.ls-tr[data-server="docker-compose-langserver"] .ls-installing');
    await page.click('.ls-tr[data-server="intelephense"]');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
    // Scroll the server table (not the page) to the Docker rows.
    await page.locator('.ls-tr[data-server="docker-compose-langserver"]').evaluate((row) => {
      let box = row.parentElement;
      while (box && box.scrollHeight <= box.clientHeight) box = box.parentElement;
      if (box) box.scrollTop += row.getBoundingClientRect().top - box.getBoundingClientRect().top - box.clientHeight / 3;
    });
    await page.waitForTimeout(600);
  }),
  "php-settings-install-intelephense": missing(async (page) => {
    await page.locator(`${OFFER} .balloon-close`).click();
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.click('.set-tree-row[data-node="lang.php"]');
    await page.waitForSelector('.php-settings button:has-text("Install Intelephense")');
  }),
  "language-server-update-offer": Object.assign(
    async (page) => {
      await page.waitForTimeout(200);
    },
    { query: "?updates=1", waitFor: '.balloon:has-text("update available")' },
  ),
  "language-servers-install-buttons-light": missing(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.locator(`${OFFER} .balloon-close`).click();
    await page.evaluate(() => window.__forgeRunCommand("langservers.settings"));
    await page.waitForSelector('[data-testid="language-servers-page"] .ls-tr[data-server]');
    await page.click('.ls-tr[data-server="vue-language-server"]');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  }),
};
