/**
 * The project lifecycle: switching the window to another project (This Window) shows the new
 * project's branch at once, a running terminal command asks before the switch, and the
 * project's terminal tabs come back when it opens again.
 */

/** @param {import("playwright-core").Page} page @param {string} path */
const openHere = (page, path) =>
  page.evaluate(async (p) => {
    const { openWorkspace } = await import("/src/state/workspace.ts");
    await openWorkspace(p, { askTrust: false });
  }, path);

/** @param {import("playwright-core").Page} page @param {string} text */
async function type(page, text) {
  await page.keyboard.type(text, { delay: 8 });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.$$(".balloon-close")) await close.click().catch(() => undefined);
}

/** @param {import("playwright-core").Page} page */
async function openTerminal(page) {
  await page.keyboard.press("Alt+F12");
  await page.waitForSelector(".term-host .xterm-screen");
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "project-switch-branch": async (page) => {
    await page.waitForSelector('.main-toolbar .pill-btn:has-text("feature/")');
    await openHere(page, "~/Projects/symfony-api");
    await page.waitForSelector('.main-toolbar .pill-btn:has-text("feature/api-v2")');
    await page.waitForSelector('.status-item:has-text("feature/api-v2")');
    await page.waitForTimeout(300);
    await closeBalloons(page);
  },

  "project-switch-no-vcs": async (page) => {
    await openHere(page, "~/.config/nvim");
    await page.waitForSelector('.main-toolbar .pill-btn:has-text("Version Control")');
    await page.waitForTimeout(300);
    await closeBalloons(page);
  },

  "terminal-terminate-on-switch": async (page) => {
    await openTerminal(page);
    await type(page, "sleep 100");
    // Not awaited: the switch waits for the answer.
    openHere(page, "~/Projects/symfony-api").catch(() => undefined);
    await page.waitForSelector('.dialog-title:has-text("Terminal Process Is Running")');
    await page.waitForTimeout(200);
  },

  "terminal-restored-tabs": async (page) => {
    await openTerminal(page);
    await type(page, "php -v");
    await page.click(".term-tabs .term-new");
    await page.waitForTimeout(250);
    await type(page, "cd app");
    await type(page, "ls");
    await openHere(page, "~/Projects/symfony-api");
    await page.waitForSelector('.main-toolbar .pill-btn:has-text("feature/api-v2")');
    await openHere(page, "demo");
    await page.waitForSelector('.term-tab[data-terminal-tab="Local (2)"]');
    await page.click('.term-tab[data-terminal-tab="Local"]');
    await page.waitForTimeout(500);
    await closeBalloons(page);
  },
};
