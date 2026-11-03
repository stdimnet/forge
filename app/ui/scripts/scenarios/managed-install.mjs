/**
 * "Click and play" npm language servers: the one-click Install buttons on the Language
 * Servers page, Forge downloading Node.js and the package with progress in the status bar,
 * and the update offer. Browser mode simulates the downloads (`?node=missing`: no Node.js
 * on the machine yet).
 */

/** Hold the simulated download at `fraction` (undefined lets it finish). */
async function holdAt(page, fraction) {
  await page.evaluate((f) => {
    window.__forgeMockInstallHold = f ?? undefined;
  }, fraction);
}

/** @param {import("playwright-core").Page} page */
async function openPage(page) {
  await page.evaluate(() => window.__forgeRunCommand("langservers.settings"));
  await page.waitForSelector('[data-testid="language-servers-page"] .ls-tr[data-server]');
}

/** @param {(page: import("playwright-core").Page) => Promise<void>} fn */
const noNode = (fn) => Object.assign(fn, { query: "?node=missing" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "managed-install-progress": noNode(async (page) => {
    await openPage(page);
    await holdAt(page, 0.42);
    await page.click('[data-testid="install-docker-compose-langserver"]');
    await page.waitForSelector('[data-testid="install-status"]:has-text("Node.js")');
    await page.waitForFunction(() => /\d+%/.test(document.querySelector('[data-testid="install-status"]')?.textContent ?? ""));
    await page.keyboard.press("Escape");
    await page.waitForTimeout(500);
  }),
  "language-servers-install-buttons": noNode(async (page) => {
    await openPage(page);
    // One server downloading, one waiting for a click.
    await holdAt(page, 0.6);
    await page.click('[data-testid="install-docker-compose-langserver"]');
    await page.waitForSelector('.ls-tr[data-server="docker-compose-langserver"] .ls-installing');
    await page.click('.ls-tr[data-server="twiggy-language-server"]');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
    // Scroll the server table (not the page) to the Docker rows.
    await page.locator('.ls-tr[data-server="docker-compose-langserver"]').evaluate((row) => {
      let box = row.parentElement;
      while (box && box.scrollHeight <= box.clientHeight) box = box.parentElement;
      if (box) box.scrollTop += row.getBoundingClientRect().top - box.getBoundingClientRect().top - box.clientHeight / 3;
    });
    await page.waitForTimeout(600);
  }),
  "language-server-update-offer": Object.assign(
    async (page) => {
      await page.waitForTimeout(200);
    },
    { query: "?updates=1", waitFor: '.balloon:has-text("update available")' },
  ),
  "language-servers-install-buttons-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPage(page);
    await page.click('.ls-tr[data-server="vue-language-server"]');
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  },
};
