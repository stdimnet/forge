/** Code host screenshots: pull request list, review tab with inline threads, create dialog, accounts. */

/** @param {import("playwright-core").Page} page */
async function openRequests(page) {
  await page.click('[data-tool-window="pull-requests"]');
  await page.waitForSelector(".fg-row[data-mr]");
}

/** @param {import("playwright-core").Page} page @param {number} n */
async function openRequest(page, n) {
  await openRequests(page);
  await page.click(`.fg-row[data-mr="${n}"]`);
  await page.waitForSelector(`[data-mr-tab="${n}"] .fg-mr-header`);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "forges-mr-list": async (page) => {
    await openRequests(page);
    await page.hover('.fg-row[data-mr="41"]');
  },
  "forges-mr-overview": async (page) => {
    await openRequest(page, 41);
    await page.waitForSelector(".fg-thread");
  },
  "forges-mr-files": async (page) => {
    await openRequest(page, 41);
    // Hide the tool window so the diff gets the full width.
    await page.click('[data-tool-window="pull-requests"]');
    await page.click('[data-view="files"]');
    await page.waitForSelector('.fg-tree-file[data-file="app/Models/User.php"]');
    await page.click('.fg-tree-file[data-file="app/Models/User.php"]');
    await page.waitForSelector(".fg-dl-thread .fg-thread");
    await page.locator(".fg-dl-thread").first().scrollIntoViewIfNeeded();
    await page.locator(".fg-diff-scroll").evaluate((el) => (el.scrollTop = Math.max(0, el.scrollTop - 80)));
  },
  "forges-create-mr": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("forges.createMergeRequest"));
    await page.waitForSelector(".fg-create select[data-field='target'] option", { state: "attached" });
    await page.fill(".fg-create textarea[data-field='description']", "Calculates order totals in `Checkout` and rounds them to cents.\n\nCloses #28");
    await page.click("[data-field='add-reviewer']");
    await page.click(".fg-picker .popup-row:has-text('Anna Weber')");
  },
  "forges-accounts": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="vcs.forgejo"]');
    await page.waitForSelector(".fg-account");
    await page.click('[data-account="forgejo-git.acme.dev"] .fg-small-btn');
    await page.waitForSelector(".fg-check.ok");
    await page.click('[data-action="add-account"]');
    await page.fill("#fg-server", "https://codeberg.org");
  },
};
