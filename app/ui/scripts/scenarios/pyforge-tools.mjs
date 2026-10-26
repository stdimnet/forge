/**
 * PyForge's run tools on the FastAPI demo (`?project=python`): a pytest run with a failed
 * parametrized case and its diff, a debugpy session with the debug console, the Python
 * Console with its variables, and the profiler's call tree of a cProfile snapshot.
 */

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1, 1);
  }, path);
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
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

/** Click the line number `line` in the gutter (toggles a breakpoint). */
async function gutter(page, line) {
  const cell = page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: new RegExp(`^${line}$`) }).first();
  await cell.click();
}

const python = (fn) => Object.assign(fn, { query: "?project=python" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // pytest on the tests folder: the JUnit report as a tree, the failed case's comparison.
  "python-pytest": python(async (page) => {
    await closeBalloons(page);
    await openFile(page, "tests/test_pricing.py");
    await page.waitForSelector(".run-gutter-icon:visible");
    await page.evaluate(async () => {
      const { launch } = await import("/src/features/run/runStore.ts");
      await launch({ name: "pytest in tests", type: "pytest", options: { scope: "directory", directory: "tests" } });
    });
    await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("Tests failed"), null, { timeout: 15000 });
    await bottomHeight(page, 360);
    await page.click('.test-row[data-test="test_round_cents[2.675-2.68]"]');
    await page.waitForTimeout(500);
  }),
  // Debug a pytest test under debugpy: paused in Cart.total, an expression in the debug console.
  "python-debug": python(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/shop_api/pricing.py");
    await gutter(page, 27);
    await page.waitForSelector(".cm-bp-dot");
    await page.evaluate(async () => {
      const { debugConfiguration } = await import("/src/features/run/runStore.ts");
      await debugConfiguration({
        name: "pytest for test_pricing.TestCart.test_total_includes_tax",
        type: "pytest",
        options: { scope: "test", file: "tests/test_pricing.py", test: "TestCart::test_total_includes_tax" },
      });
    });
    await page.waitForSelector(".cm-exec-line", { timeout: 15000 });
    await bottomHeight(page, 320);
    await page.evaluate(async () => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt("src/shop_api/pricing.py", 30, 1);
    });
    await page.click('.dbg-tab[data-tab="console"]');
    const input = page.locator(".dbg-console-input .dbg-expr-input");
    await input.click();
    await input.fill("net * (1 + tax_rate)");
    await input.press("Enter");
    await page.waitForSelector(".dbg-console-value .dbg-row");
    await input.fill("self.items");
    await input.press("Enter");
    await page.waitForFunction(() => document.querySelectorAll(".dbg-console-value .dbg-row").length >= 2);
    await page.waitForTimeout(300);
  }),
  // The Python Console: Run Selection in Python Console, then a typed expression; variables on the right.
  "python-console": python(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/shop_api/pricing.py");
    await page.evaluate(async () => {
      const { executeCode } = await import("/src/features/python/console/store.ts");
      await executeCode('from decimal import Decimal\nfrom shop_api.pricing import Cart, round_cents\ncart = Cart()\ncart.add("mug", Decimal("12.50"), 2)\ncart.total()\n');
    });
    await page.waitForSelector('[data-testid="python-console-variables"] .pycon-var[data-expr="cart"]');
    await bottomHeight(page, 340);
    const input = page.locator('[data-testid="python-console-input"]');
    await input.click();
    await input.fill("round_cents(2.675)");
    await input.press("Enter");
    await page.waitForTimeout(400);
    await page.click('.pycon-var[data-expr="cart"] .pycon-var-toggle');
    await page.waitForSelector('.pycon-var[data-expr="cart.items"]');
    await page.waitForTimeout(300);
  }),
  // Profile 'report': the cProfile snapshot's call tree.
  "python-profiler": python(async (page) => {
    await closeBalloons(page);
    await openFile(page, "scripts/report.py");
    await page.evaluate(async () => {
      const { launch } = await import("/src/features/run/runStore.ts");
      await launch({ name: "report", type: "python", options: { target: "script", script: "scripts/report.py" } }, { extra: { profile: true } });
    });
    await page.waitForSelector('[data-testid="profiler-call-tree"] .prof-row', { timeout: 15000 });
    await bottomHeight(page, 380);
    await page.waitForTimeout(400);
  }),
  "python-profiler-hot-spots": python(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { launch } = await import("/src/features/run/runStore.ts");
      await launch({ name: "report", type: "python", options: { target: "script", script: "scripts/report.py" } }, { extra: { profile: true } });
    });
    await page.waitForSelector('[data-testid="profiler-call-tree"] .prof-row', { timeout: 15000 });
    await page.click('.prof-tab:has-text("Method List")');
    await page.waitForSelector('[data-testid="profiler-hot-spots"] .prof-row');
    await bottomHeight(page, 380);
    await page.waitForTimeout(300);
  }),
};
