/**
 * Web debugging and debugger tools: console with completion, Set Value, logpoints,
 * Web Server Debug Validation, PHP Built-in Web Server / PHP Web Page configurations and
 * Debug in .http files.
 */

/** @param {import("playwright-core").Page} page */
async function openController(page) {
  for (const dir of ["app/Http", "app/Http/Controllers"]) {
    await page.click(`.tree-row[data-path="${dir}"]`);
  }
  await page.click('.tree-row[data-path="app/Http/Controllers/OrderController.php"]');
  await page.waitForSelector(".cm-bp-dot");
}

/** @param {import("playwright-core").Page} page */
async function tallerBottom(page) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (box) {
    await page.mouse.move(box.x + 300, box.y + 1);
    await page.mouse.down();
    await page.mouse.move(box.x + 300, box.y - 150, { steps: 6 });
    await page.mouse.up();
  }
}

/** @param {import("playwright-core").Page} page */
async function startPaused(page) {
  await closeBalloons(page);
  await openController(page);
  await page.click(".dbg-listen-btn");
  await page.waitForSelector(".cm-exec-line");
  await page.waitForSelector('.dbg-row[data-var="$order"]');
  await tallerBottom(page);
}

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "debugger-console": async (page) => {
    await startPaused(page);
    await page.click('.dbg-tab:has-text("Console")');
    const input = page.locator(".dbg-console-input .dbg-expr-input");
    await input.click();
    await input.fill("$order->attributes['total'] * 0.9");
    await input.press("Enter");
    await page.waitForSelector(".dbg-console-value .dbg-row");
    await input.fill("$order");
    await input.press("Enter");
    await page.waitForFunction(() => document.querySelectorAll(".dbg-console-value .dbg-row").length >= 2);
    await input.pressSequentially("$order->st", { delay: 30 });
    await page.waitForSelector(".dbg-completion");
    await page.waitForTimeout(150);
  },
  "debugger-set-value": async (page) => {
    await startPaused(page);
    await page.click('.dbg-row[data-var="$order"] .tree-chevron');
    await page.waitForSelector('.dbg-row[data-var="$order->attributes"]');
    await page.click('.dbg-row[data-var="$order->attributes"] .tree-chevron');
    const status = page.locator(`.dbg-row[data-var="$order->attributes['status']"]`);
    await status.waitFor();
    await status.click();
    await page.keyboard.press("F2");
    await page.waitForSelector(".dbg-value-edit");
    await page.fill(".dbg-value-edit", "'paid'");
  },
  "debugger-variable-menu": async (page) => {
    await startPaused(page);
    await page.click('.dbg-row[data-var="$order"] .tree-chevron');
    const row = page.locator('.dbg-row[data-var="$order->attributes"]');
    await row.waitFor();
    await row.click({ button: "right" });
    await page.waitForSelector('.run-popup-menu .run-menu-item:has-text("Set Value")');
  },
  "debugger-logpoint": async (page) => {
    await closeBalloons(page);
    await openController(page);
    await page.keyboard.press("Control+Shift+F8");
    await page.waitForSelector(".dbg-breakpoints");
    await page.click('.dbg-bp-item:has-text("OrderController.php:22")');
    await page.waitForSelector("#dbg-bp-log-dlg");
  },
  "debugger-logpoint-output": async (page) => {
    await startPaused(page);
    await page.click('.dbg-tab:has-text("Console")');
    await page.waitForSelector(".dbg-out-log");
  },
  "debugger-validation": async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("WebServerDebugValidation"));
    await page.waitForSelector(".dbg-validate");
    await page.click('.dbg-validate .btn-primary:has-text("Validate")');
    await page.waitForSelector(".dbg-check-row");
  },
  "run-builtin-server": async (page) => {
    await closeBalloons(page);
    await page.click(".run-config");
    await page.click(".run-dropdown .run-dd-item:has-text('Edit Configurations')");
    await page.waitForSelector(".rc-dialog");
    await page.click('.rc-tree-item[data-config="localhost:8080"]');
    await page.waitForTimeout(200);
  },
  "run-web-page": async (page) => {
    await closeBalloons(page);
    await page.click(".run-config");
    await page.click(".run-dropdown .run-dd-item:has-text('Edit Configurations')");
    await page.waitForSelector(".rc-dialog");
    await page.click('.rc-tree-item[data-config="Order page"]');
    await page.waitForTimeout(200);
  },
  "http-debug-menu": async (page) => {
    await closeBalloons(page);
    const app = page.locator('.tree-row[data-path="app"]');
    if ((await app.getAttribute("data-expanded")) !== null) await app.locator(".tree-chevron").click();
    await page.locator('.tree-row[data-path="scratches-and-consoles://"] .tree-chevron').click();
    await page.locator('.tree-row[data-path="scratches-and-consoles://scratches"] .tree-chevron').click();
    await page.locator('.tree-row[data-path="scratch://rest-api.http"]').click();
    await page.waitForSelector(".cm-line-marker.marker-run");
    await page.waitForTimeout(150);
    await page.locator(".cm-line-marker.marker-run").nth(1).click();
    await page.waitForSelector('.run-popup-menu .run-menu-item:has-text("with Xdebug")');
  },
  "http-debug-run": async (page) => {
    await closeBalloons(page);
    const app = page.locator('.tree-row[data-path="app"]');
    if ((await app.getAttribute("data-expanded")) !== null) await app.locator(".tree-chevron").click();
    await page.locator('.tree-row[data-path="scratches-and-consoles://"] .tree-chevron').click();
    await page.locator('.tree-row[data-path="scratches-and-consoles://scratches"] .tree-chevron').click();
    await page.locator('.tree-row[data-path="scratch://rest-api.http"]').click();
    await page.waitForSelector(".cm-line-marker.marker-run");
    await page.waitForTimeout(150);
    await page.locator(".cm-line-marker.marker-run").nth(2).click();
    await page.click('.run-popup-menu .run-menu-item:has-text("with Xdebug")');
    await page.waitForFunction(() => [...document.querySelectorAll(".run-line")].some((l) => l.textContent?.includes("XDEBUG_SESSION=FORGE")), null, {
      timeout: 10000,
    });
    await page.locator(".run-console").evaluate((el) => (el.scrollTop = 0));
  },
  "debugger-light-console": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await startPaused(page);
    await page.click('.dbg-tab:has-text("Console")');
    const input = page.locator(".dbg-console-input .dbg-expr-input");
    await input.fill("count($lines->items)");
    await input.press("Enter");
    await page.waitForSelector(".dbg-out-input");
    await input.pressSequentially("$", { delay: 30 });
    await page.waitForSelector(".dbg-completion");
  },
};
