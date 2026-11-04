/**
 * Agent Tasks (browser mode: a scripted fake agent): the task list with a finished task to
 * review, the New Agent Task form, a command waiting for approval while a task runs, the diff
 * of a changed file and the light theme.
 */

/** Close unrelated balloons. @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  await page.waitForTimeout(1500);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @param {import("playwright-core").Page} page */
async function open(page) {
  await closeBalloons(page);
  await page.click('[data-tool-window="agent-tasks"]');
  await page.waitForSelector(".at-panel .at-row");
  await page.waitForSelector(".at-files .at-file");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "agent-tasks-review": async (page) => {
    await open(page);
    await page.waitForTimeout(300);
  },

  "agent-tasks-new": async (page) => {
    await open(page);
    await page.click('.tool-window [title="New Agent Task"]');
    await page.waitForSelector(".at-form textarea");
    await page.fill(".at-form textarea", "Make PriceCalculator round discounts to cents and add tests for coupons.");
    await page.fill("#at-scope", "app/Services, tests/Unit");
    await page.selectOption("#at-tests", "command");
    await page.fill('.at-form input[placeholder="vendor/bin/phpunit"]', "vendor/bin/phpunit tests/Unit");
    await page.waitForTimeout(200);
  },

  "agent-tasks-approval": async (page) => {
    await open(page);
    await page.click('.tool-window [title="New Agent Task"]');
    await page.fill(".at-form textarea", "Add Checkout::total() so the CheckoutTest passes.");
    await page.selectOption("#at-tests", "command");
    await page.fill('.at-form input[placeholder="vendor/bin/phpunit"]', "vendor/bin/phpunit tests/Unit/CheckoutTest.php");
    await page.click(".at-form .btn-primary");
    await page.waitForSelector(".at-approval .btn-primary", { timeout: 20000 });
    await closeBalloons(page);
    await page.evaluate(() => document.querySelector(".at-approval")?.scrollIntoView({ block: "center" }));
    await page.waitForTimeout(200);
  },

  "agent-tasks-diff": async (page) => {
    await open(page);
    await page.dblclick(".at-files .at-file");
    await page.waitForSelector(".cm-mergeView");
    await page.waitForTimeout(400);
  },

  "agent-tasks-container": async (page) => {
    await open(page);
    await page.click('.tool-window [title="New Agent Task"]');
    await page.waitForSelector(".at-form textarea");
    await page.fill(".at-form textarea", "Fix the failing OrderTest; run the tests in a container.");
    await page.selectOption("#at-tests", "command");
    await page.fill('.at-form input[placeholder="vendor/bin/phpunit"]', "vendor/bin/phpunit");
    await page.check('[data-testid="at-container"]');
    await page.check('[data-testid="at-network"]');
    await page.waitForTimeout(200);
  },

  "agent-tasks-container-log": async (page) => {
    await open(page);
    await page.evaluate(() => document.querySelector(".at-steps")?.scrollIntoView({ block: "start" }));
    await page.waitForTimeout(300);
  },

  "agent-tasks-container-de": async (page) => {
    await page.evaluate(async () => {
      const i18n = await import("/src/i18n/index.ts");
      await i18n.setLocale?.("de");
    });
    await open(page);
    await page.click('.tool-window [title="New Agent Task"]');
    await page.waitForSelector(".at-form textarea");
    await page.check('[data-testid="at-container"]');
    await page.waitForTimeout(300);
  },

  "agent-tasks-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await open(page);
    await page.waitForTimeout(300);
  },
};
