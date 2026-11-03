/** Review mode of a pull request: viewed files, pending comments, AI summary and findings. */

/** @param {import("playwright-core").Page} page @param {number} n */
async function openRequest(page, n) {
  await page.addStyleTag({ content: ".balloons { display: none !important; }" });
  await page.click('[data-tool-window="pull-requests"]');
  await page.waitForSelector(`.fg-row[data-mr="${n}"]`);
  await page.click(`.fg-row[data-mr="${n}"]`);
  await page.waitForSelector(`[data-mr-tab="${n}"] .fg-mr-header`);
  // Hide the tool window so the review gets the full width.
  await page.click('[data-tool-window="pull-requests"]');
}

/** @param {import("playwright-core").Page} page */
async function summarizeAndReview(page) {
  await page.click('[data-view="ai"]');
  await page.waitForSelector("[data-ai-bar]");
  await page.click('[data-action="summarize"]');
  await page.click('[data-action="ai-review"]');
  await page.waitForSelector(".fg-test-plan li");
  await page.waitForSelector("[data-finding]");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "code-review-ai-summary": async (page) => {
    await openRequest(page, 41);
    await summarizeAndReview(page);
  },
  "code-review-files": async (page) => {
    await openRequest(page, 41);
    await summarizeAndReview(page);
    // One finding becomes a pending comment, one is dismissed.
    await page.click('[data-finding*="tests/Unit/UserTest.php"] [data-action="finding-draft"]');
    await page.click('[data-finding*="2026_10_14"] [data-action="finding-dismiss"]');
    await page.click('[data-view="files"]');
    await page.waitForSelector('[data-viewed="app/Enums/UserRole.php"]');
    await page.click('[data-viewed="app/Enums/UserRole.php"]');
    await page.click('[data-viewed="database/migrations/2026_10_14_000000_add_role_to_users.php"]');
    await page.click('.fg-tree-file[data-file="app/Models/User.php"]');
    await page.waitForSelector(".fg-dl-thread [data-finding]");
    await page.locator('[data-action="explain-hunk"]').nth(1).click({ force: true });
    await page.waitForSelector(".fg-explain [data-ai-provider]");
    await page.locator(".fg-dl-thread [data-finding]").first().scrollIntoViewIfNeeded();
    await page.locator(".fg-explain").first().scrollIntoViewIfNeeded();
    await page.locator(".fg-diff-scroll").evaluate((el) => (el.scrollTop = Math.max(0, el.scrollTop - 40)));
  },
  "code-review-submit": async (page) => {
    await openRequest(page, 41);
    await page.click('[data-view="files"]');
    await page.click('.fg-tree-file[data-file="app/Models/User.php"]');
    await page.waitForSelector("tr.fg-dl-added");
    // A range comment: click + on one line, Shift+click + on another line of the hunk.
    const added = page.locator("tr.fg-dl-added");
    await added.nth(1).hover();
    await added.nth(1).locator(".fg-dl-add").click();
    await page.fill(".fg-thread-new textarea", "Please add a test for `isAdmin()` with a customer.");
    await page.click(".fg-thread-new .btn-primary");
    await page.waitForSelector("[data-draft]");
    await page.click('[data-action="submit-review"]');
    await page.waitForSelector("[data-submit-popup]");
    await page.fill("[data-submit-popup] textarea", "Looks good apart from the role cast. See the comments.");
    await page.click("[data-submit-popup] input[type=radio] >> nth=2");
  },
  "code-review-ai-summary-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openRequest(page, 41);
    await summarizeAndReview(page);
  },
};
