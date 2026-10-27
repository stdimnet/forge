/**
 * Code coverage beyond the basics: the PHP coverage driver in the Run/Debug Configurations
 * dialog, per-test coverage in the editor, Istanbul JSON branch coverage of a Vitest run (the
 * Node.js demo), the suite chooser merging two suites and the generated HTML report.
 */

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1, 1);
  }, path);
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(300);
}

/** Run a configuration with coverage and wait for its suite in the Coverage tool window. */
async function runWithCoverage(page, cfg) {
  await page.evaluate(async (c) => {
    const { runWithCoverage } = await import("/src/features/run/runStore.ts");
    await runWithCoverage(c);
  }, cfg);
  await page.waitForFunction((name) => document.querySelector(".coverage-title b")?.textContent === name, cfg.name, { timeout: 20000 });
  await page.evaluate(async () => {
    const { resizePanel } = await import("/src/state/layout.ts");
    resizePanel("right", 640);
  });
  await page.waitForTimeout(300);
}

async function dismissBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

const PER_TEST = { name: "Unit (PCOV, per test)", type: "phpunit", options: { scope: "config", coverageDriver: "pcov", perTestCoverage: "true" } };

/** Run/Debug Configurations: the Code Coverage group of a PHPUnit configuration. */
async function driver(page) {
  await page.evaluate(async () => {
    const { openRunConfigurations } = await import("/src/features/run/dialogState.ts");
    openRunConfigurations("All tests");
  });
  await page.waitForSelector('[data-testid="rc-coverage"]');
  await page.click('[data-testid="rc-coverage"] label.rc-radio:has-text("Xdebug") input');
  await page.click('[data-testid="rc-coverage"] label:has-text("Branch and path coverage") input');
  await page.click('[data-testid="rc-coverage"] label:has-text("Track per-test coverage") input');
  await page.locator('[data-testid="rc-coverage"]').scrollIntoViewIfNeeded();
  await page.waitForTimeout(300);
}

/** PHPUnit with PCOV and per-test coverage: the tests covering a line. */
async function perTest(page) {
  await runWithCoverage(page, PER_TEST);
  await openFile(page, "app/Services/InvoiceService.php");
  await page.waitForSelector(".editor-slot:not(.hidden) .coverage-mark.with-tests");
  await dismissBalloons(page);
  const mark = page.locator(".editor-slot:not(.hidden) .coverage-mark.with-tests").nth(3);
  await mark.click();
  await page.waitForSelector(".coverage-line-popup");
  await page.waitForTimeout(300);
}

const node = (fn) => Object.assign(fn, { query: "?project=node" });

/** Vitest with the istanbul provider: Istanbul JSON with branches and functions. */
async function istanbul(page) {
  await runWithCoverage(page, { name: "Vitest (istanbul)", type: "vitest", options: { scope: "all", coverageProvider: "istanbul" } });
  await openFile(page, "src/orders/vouchers.ts");
  await page.waitForSelector(".editor-slot:not(.hidden) .coverage-mark.partial");
  await dismissBalloons(page);
  await page.waitForTimeout(300);
}

/** Show Coverage Data: two suites checked, merged into one. */
async function suites(page) {
  await runWithCoverage(page, { name: "All tests", type: "phpunit", options: { scope: "config" } });
  await runWithCoverage(page, PER_TEST);
  await dismissBalloons(page);
  await page.evaluate(() => window.__forgeRunCommand("SwitchCoverage"));
  await page.waitForSelector(".coverage-suites-dialog .coverage-suite-item");
  const boxes = page.locator(".coverage-suites-dialog .coverage-suite-item input");
  for (let i = 0; i < (await boxes.count()); i++) if (!(await boxes.nth(i).isChecked())) await boxes.nth(i).check();
  await page.waitForTimeout(300);
}

/** The merged suite in the tool window with the Method column. */
async function merged(page) {
  await suites(page);
  await page.click('.coverage-suites-dialog button:has-text("Show Selected")');
  await page.waitForFunction(() => document.querySelector(".coverage-title b")?.textContent?.includes(" + "));
  // Folded folder percentages in the Project view.
  for (const dir of ["app", "app/Services"]) {
    const row = page.locator(`.tree-row[data-path="${dir}"]`).first();
    if (await row.count()) {
      const open = await row.evaluate((el) => el.getAttribute("aria-expanded"));
      if (open !== "true") await row.dblclick();
      await page.waitForTimeout(200);
    }
  }
  await page.waitForTimeout(300);
}

/** Generate Coverage Report: the HTML rendered from the model, shown in the page. */
async function htmlReport(page) {
  await runWithCoverage(page, PER_TEST);
  const html = await page.evaluate(async () => {
    const { testingApi } = await import("/src/ipc/testing.ts");
    const { activeSuite } = await import("/src/features/coverage/store.ts");
    return testingApi.coverageExportHtml(activeSuite().id);
  });
  await page.setContent(html);
  await page.waitForTimeout(200);
}

export default {
  "coverage-driver": driver,
  "coverage-per-test": perTest,
  "coverage-istanbul": node(istanbul),
  "coverage-suites": suites,
  "coverage-merged": merged,
  "coverage-html-report": htmlReport,
};
