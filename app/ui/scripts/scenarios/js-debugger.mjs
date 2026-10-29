/**
 * The JavaScript debugger (js-debug) and the Node.js test runners in the Node.js demo
 * (`?project=node`): a Node.js debug session stopped at a breakpoint in TypeScript, the
 * JavaScript Debug run configuration, the js-debug download, a Vitest test tree and
 * Playwright tests.
 */

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

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

/** Click the line number `line` in the gutter (toggles a breakpoint). */
async function toggleBreakpoint(page, line) {
  const cell = page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: new RegExp(`^${line}$`) }).first();
  await cell.click();
  await page.waitForSelector(".cm-bp-dot");
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
  await page.click(`.run-popup-menu .run-menu-item:has-text("${item}")`);
}

async function waitForTests(page, text = "Tests failed") {
  await page.waitForSelector(".run-tests-status");
  await page.waitForFunction((t) => document.querySelector(".run-tests-status")?.textContent?.includes(t), text, { timeout: 15000 });
}

/** Debug the selected configuration ("server": src/server.ts with tsx). */
async function debugServer(page) {
  await page.evaluate(async () => {
    const { debugConfiguration } = await import("/src/features/run/runStore.ts");
    const { findConfiguration } = await import("/src/features/run/configStore.ts");
    await debugConfiguration(findConfiguration("server"));
  });
}

const node = (fn) => Object.assign(fn, { query: "?project=node" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "js-debug-node-typescript": node(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/orders/pricing.ts");
    await toggleBreakpoint(page, 12);
    await debugServer(page);
    await page.waitForSelector(".cm-exec-line", { timeout: 15000 });
    await page.waitForSelector('.dbg-row[data-var="order"]');
    await bottomHeight(page, 330);
    await page.click('.dbg-row[data-var="order"] .tree-chevron');
    await page.waitForSelector('.dbg-row[data-var="order.voucher"]').catch(() => {});
    await page.waitForTimeout(400);
  }),
  "js-debug-chrome-configuration": node(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openRunConfigurations } = await import("/src/features/run/dialogState.ts");
      openRunConfigurations("Order 1042 in Chrome");
    });
    await page.waitForSelector(".rc-dialog, .run-config-dialog, [data-testid='rc-dialog']", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(600);
  }),
  "js-debug-download": node(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { registerMock } = await import("/src/ipc/index.ts");
      const status = {
        server: null,
        custom: false,
        node: "/usr/bin/node",
        release: {
          version: "1.140.0",
          url: "https://github.com/microsoft/vscode-js-debug/releases/download/v1.140.0/js-debug-dap-v1.140.0.tar.gz",
          sha256: "27dab92937ec1ab35821ae955aac867544fe06a1b6307229049f2d789af10968",
          size: 1249709,
        },
      };
      registerMock("js_debug_status", () => status);
      const { ensureJsDebug } = await import("/src/features/webforge/jsDebug.ts");
      void ensureJsDebug();
    });
    await page.waitForSelector(".bc-confirm");
    await page.waitForTimeout(300);
  }),
  "js-tests-vitest": node(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/orders/pricing.test.ts");
    await gutterRun(page, 'describe("pricing"', "Run 'pricing'");
    await waitForTests(page);
    await page.waitForTimeout(600);
    await bottomHeight(page, 330);
    await page.click('.test-row[data-test="applies the voucher before the tax"]');
    await page.waitForTimeout(300);
  }),
  "js-tests-playwright": node(async (page) => {
    await closeBalloons(page);
    await openFile(page, "e2e/checkout.spec.ts");
    await gutterRun(page, "test.describe(", "Run 'order totals'");
    await waitForTests(page);
    await page.waitForTimeout(600);
    await bottomHeight(page, 330);
    await page.click('.test-row[data-test="returns the total of an order"]');
    await page.waitForTimeout(300);
  }),
};
