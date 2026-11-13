/** Git integration screenshots: commit window, diff, blame, log, branches popup. */

/** @param {import("playwright-core").Page} page */
async function openCommit(page) {
  await page.keyboard.press("Control+k");
  await page.waitForSelector(".commit-row");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "vcs-commit": async (page) => {
    await openCommit(page);
    // Wider panel so the generated message reads naturally.
    await page.mouse.move(340, 400);
    await page.mouse.down();
    await page.mouse.move(470, 400, { steps: 4 });
    await page.mouse.up();
    await page.click(".ai-generate");
    await page.waitForFunction(() => document.querySelector(".commit-message textarea")?.value.length > 0);
    await page.click('.commit-row[data-path="app/Models/User.php"]');
  },
  "vcs-diff": async (page) => {
    await openCommit(page);
    await page.dblclick('.commit-row[data-path="app/Models/User.php"]');
    await page.waitForSelector(".diff-viewer .cm-mergeView");
    await page.waitForTimeout(200);
  },
  "vcs-blame": async (page) => {
    await page.locator(".cm-line").nth(33).click();
    await page.locator(".cm-lineNumbers").click({ button: "right", position: { x: 20, y: 60 } });
    await page.click("text=Annotate with Git Blame");
    const annotated = ".cm-git-blame:not(.cm-git-blame-spacer):has(.cm-git-blame-date)";
    await page.waitForSelector(annotated);
    await page.locator(annotated).nth(4).hover();
    await page.waitForSelector(".blame-card");
  },
  "vcs-log": async (page) => {
    await page.keyboard.press("Alt+9");
    await page.waitForSelector(".log-row");
    await page.locator(".bottom-panel").evaluate(() => undefined);
    // Give the bottom panel more room, like a typical log layout.
    await page.mouse.move(700, 900 - 26 - 241);
    await page.mouse.down();
    await page.mouse.move(700, 330, { steps: 4 });
    await page.mouse.up();
    await page.locator(".log-row").nth(1).click();
    await page.waitForSelector(".log-subject");
  },
  "vcs-branches": async (page) => {
    await page.click('.main-toolbar [title="Git Branches"]');
    await page.waitForSelector(".branch-row");
    await page.locator(".popup-section:text('Local') ~ .branch-row").first().waitFor();
    const row = page.locator(".branch-row", { hasText: "main" }).first();
    await row.hover();
    await page.waitForSelector(".branch-submenu");
  },
};
