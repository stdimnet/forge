/**
 * The shared test runner of every pack: a busted (LuaForge, TAP) test tree with a failed
 * comparison, Run with Coverage of the PHP demo (Coverage tool window, editor gutter marks,
 * Project view hints) and the "Run on" selector of a run configuration.
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

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + 200, y);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, 900 - 26 - height, { steps: 4 });
  await page.mouse.up();
}

/** Click the gutter icon on the line containing `text` and pick a menu item. */
async function gutterRun(page, text, item) {
  const line = page.locator(".cm-line:visible", { hasText: text }).first();
  const box = await line.boundingBox();
  const icons = page.locator(".cm-run-gutter:visible .cm-gutterElement");
  const count = await icons.count();
  for (let i = 0; i < count; i++) {
    const b = await icons.nth(i).boundingBox();
    if (b && box && Math.abs(b.y - box.y) < 4 && (await icons.nth(i).locator(".run-gutter-icon").count())) {
      await icons.nth(i).locator(".run-gutter-icon").click();
      break;
    }
  }
  await page.click(`.run-popup-menu .run-menu-item:has-text("${item}")`);
}

async function waitForTests(page, text = "Tests failed") {
  await page.waitForSelector(".run-tests-status");
  await page.waitForFunction((t) => document.querySelector(".run-tests-status")?.textContent?.includes(t), text, { timeout: 15000 });
}

const lua = (fn) => Object.assign(fn, { query: "?project=lua" });

/** busted specs from the gutter: the TAP output as a test tree, the failed comparison selected. */
async function bustedRun(page) {
  await openFile(page, "spec/player_spec.lua");
  await page.waitForSelector(".run-gutter-icon:visible");
  await gutterRun(page, 'describe("Player"', "Run 'Player'");
  await waitForTests(page);
  await page.waitForTimeout(600);
  await bottomHeight(page, 330);
  await page.click('.test-row[data-test="Player moves by speed and frame time"]');
  await page.waitForTimeout(300);
}

/** The gutter menu of a busted test: Run, Run with Coverage, Debug. */
async function bustedGutterMenu(page) {
  await openFile(page, "spec/entity_spec.lua");
  await page.waitForSelector(".run-gutter-icon:visible");
  const line = page.locator(".cm-line:visible", { hasText: 'it("never goes below zero"' }).first();
  const box = await line.boundingBox();
  const icons = page.locator(".cm-run-gutter:visible .cm-gutterElement");
  for (let i = 0; i < (await icons.count()); i++) {
    const b = await icons.nth(i).boundingBox();
    if (b && box && Math.abs(b.y - box.y) < 4 && (await icons.nth(i).locator(".run-gutter-icon").count())) {
      await icons.nth(i).locator(".run-gutter-icon").click();
      break;
    }
  }
  await page.waitForSelector(".run-popup-menu");
  await page.waitForTimeout(200);
}

/** Run with Coverage of the PHPUnit configuration: tool window, gutter marks and Project view hints. */
async function coverage(page) {
  await page.evaluate(() => window.__forgeRunCommand("Coverage"));
  await page.waitForSelector('[data-tool-window-panel="coverage"] .coverage-row', { timeout: 20000 });
  await page.evaluate(async () => {
    const { resizePanel } = await import("/src/state/layout.ts");
    resizePanel("right", 520);
  });
  await openFile(page, "app/Services/InvoiceService.php");
  await page.waitForSelector(".editor-slot:not(.hidden) .coverage-mark");
  // Expand app/Services in the Project view to show the hints.
  for (const dir of ["app", "app/Services"]) {
    const row = page.locator(`.tree-row[data-path="${dir}"]`).first();
    if (await row.count()) {
      const open = await row.evaluate((el) => el.getAttribute("aria-expanded"));
      if (open !== "true") await row.dblclick();
      await page.waitForTimeout(200);
    }
  }
  await page.click('.coverage-row[data-coverage-path="app/Services/InvoiceService.php"]');
  // Dismiss unrelated balloons (the demo's ".env" database hint).
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
  await page.waitForTimeout(400);
}

/** The same in the light theme. */
async function coverageLight(page) {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await coverage(page);
}

/** "Run on: Docker image" of a new busted configuration in the Run/Debug Configurations dialog. */
async function runOn(page) {
  await page.evaluate(async () => {
    const { openRunConfigurations } = await import("/src/features/run/dialogState.ts");
    openRunConfigurations();
  });
  await page.waitForSelector(".rc-dialog");
  await page.click('button[title^="Add New Configuration"]');
  await page.click('.run-popup-menu .run-menu-item:has-text("busted")');
  await page.waitForSelector('[data-testid="rc-run-on"]');
  await page.selectOption('[data-testid="rc-run-on"]', "dockerImage");
  await page.fill('input[name="runOn.image"]', "nickblah/lua:5.4-luarocks");
  await page.press('input[name="runOn.image"]', "Tab");
  await page.fill('input[name="runOn.runOptions"]', "--network host");
  await page.press('input[name="runOn.runOptions"]', "Tab");
  await page.waitForTimeout(400);
}

export default {
  "test-runner-busted": lua(bustedRun),
  "test-runner-gutter-menu": lua(bustedGutterMenu),
  "coverage-tool-window": coverage,
  "coverage-light": coverageLight,
  "run-on-selector": lua(runOn),
};
