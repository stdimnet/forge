/**
 * GoForge screenshots, on the `ledger` demo module (`?project=go`): the Go editor with test
 * gutter icons, go.mod with version completion, a go test tree with a failing subtest, a
 * Delve debug session, golangci-lint problems, the pprof Profiler and Settings | Go.
 */

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path, line = 1) {
  await page.evaluate(
    async ([p, l]) => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt(p, l, 1, { flash: false });
    },
    [path, line],
  );
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
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

/** Click the gutter ▶ icon on the line containing `text` and pick a menu item. */
async function gutterRun(page, text, item) {
  await page.waitForSelector(".run-gutter-icon:visible");
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
  if (item) await page.click(`.run-popup-menu .run-menu-item:has-text("${item}")`);
}

const go = (fn) => Object.assign(fn, { query: "?project=go" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // A test file: ▶ on the package, the tests, subtests, the benchmark, fuzz test and example.
  "go-editor": go(async (page) => {
    await closeBalloons(page);
    await openFile(page, "internal/money/money_test.go");
    await page.waitForSelector(".run-gutter-icon:visible");
    await gutterRun(page, 't.Run("negative"');
    await page.waitForSelector(".run-popup-menu");
    await page.waitForTimeout(300);
  }),
  // go.mod: directives, module paths and versions highlighted; versions completed from the proxy.
  "go-mod": go(async (page) => {
    await closeBalloons(page);
    await openFile(page, "go.mod");
    const line = page.locator(".cm-line", { hasText: "github.com/google/uuid" }).first();
    await line.click();
    await page.keyboard.press("End");
    for (let i = 0; i < 5; i++) await page.keyboard.press("Backspace");
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(400);
  }),
  // All tests: the go test -json tree with the failing subtest selected.
  "go-test-tree": go(async (page) => {
    await closeBalloons(page);
    await openFile(page, "internal/money/money_test.go");
    await gutterRun(page, "func TestRound", "Run 'TestRound'");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("failed"), null, { timeout: 15000 });
    await page.waitForTimeout(600);
    await bottomHeight(page, 330);
    await page.click('.test-row[data-test="TestRound/negative"]').catch(() => {});
    await page.waitForTimeout(400);
  }),
  // Delve: Debug "ledger" stopped at a breakpoint in report.Total.
  "go-debug": go(async (page) => {
    await closeBalloons(page);
    await openFile(page, "internal/report/report.go", 22);
    const cell = page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: /^22$/ }).first();
    await cell.click();
    await page.waitForSelector(".cm-bp-dot");
    await page.evaluate(async () => {
      const { debugConfiguration } = await import("/src/features/run/runStore.ts");
      const { findConfiguration } = await import("/src/features/run/configStore.ts");
      await debugConfiguration(findConfiguration("ledger"));
    });
    await page.waitForSelector(".cm-exec-line", { timeout: 15000 });
    await page.waitForSelector('.dbg-row[data-var="e"]', { timeout: 5000 }).catch(() => {});
    await bottomHeight(page, 330);
    await page.click('.dbg-row[data-var="e"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(500);
  }),
  // golangci-lint: problems of the project in the editor and the Problems tool window.
  "go-lint": go(async (page) => {
    await closeBalloons(page);
    await openFile(page, "internal/store/store.go", 24);
    await page.evaluate(() => window.__forgeRunCommand("go.golangciLint"));
    await page.waitForSelector('[data-tool-window-panel="problems"]');
    await page.waitForTimeout(1200);
    await closeBalloons(page);
  }),
  // The Profiler with the CPU profile of BenchmarkReport.
  "go-profiler": go(async (page) => {
    await closeBalloons(page);
    await openFile(page, "internal/report/report.go", 29);
    await page.evaluate(() => window.__forgeOpenProfile("/home/anna/.cache/forge/go-profiles/BenchmarkReport-cpu.pprof", "BenchmarkReport (cpu)"));
    await page.waitForSelector('[data-testid="profiler-flame"]');
    await bottomHeight(page, 380);
    await page.waitForTimeout(500);
  }),
  "go-profiler-tree": go(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeOpenProfile("/home/anna/.cache/forge/go-profiles/BenchmarkReport-cpu.pprof", "BenchmarkReport (cpu)"));
    await page.waitForSelector('[data-testid="profiler-flame"]');
    await bottomHeight(page, 380);
    await page.click('.goprof-tabs button:has-text("Call Tree")');
    await page.waitForTimeout(400);
  }),
  // Settings | Languages & Frameworks | Go.
  "go-settings": go(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.go"]');
    await page.waitForSelector('[data-testid="go-sdk"]');
    await page.waitForTimeout(800);
  }),
};
