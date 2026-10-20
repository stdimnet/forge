/** PHP debugger (Xdebug) screenshots: paused session, dialogs, settings. */

/** @param {import("playwright-core").Page} page */
async function openController(page) {
  for (const dir of ["app/Http", "app/Http/Controllers"]) {
    await page.dblclick(`.tree-row[data-path="${dir}"]`);
  }
  await page.dblclick('.tree-row[data-path="app/Http/Controllers/OrderController.php"]');
  await page.waitForSelector(".cm-bp-dot");
}

/** Listen, let the mocked request hit the breakpoint and wait for the variables. */
async function startPaused(page) {
  await openController(page);
  await page.click(".dbg-listen-btn");
  await page.waitForSelector(".cm-exec-line");
  await page.waitForSelector('.dbg-row[data-var="$order"]');
  await page.waitForSelector(".cm-debug-inline");
  // A taller bottom panel, like a typical debugging layout.
  const box = await page.locator(".splitter-h").boundingBox();
  if (box) {
    await page.mouse.move(box.x + 300, box.y + 1);
    await page.mouse.down();
    await page.mouse.move(box.x + 300, box.y - 150, { steps: 6 });
    await page.mouse.up();
  }
}

/** @param {import("playwright-core").Page} page @param {string} id */
async function openPhpSettings(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click('.set-tree-row[data-node="lang.php"] .set-tree-chevron');
  await page.click(`.set-tree-row[data-node="${id}"]`);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "debugger-paused": async (page) => {
    await startPaused(page);
    await page.click('.dbg-row[data-var="$order"] .tree-chevron');
    await page.waitForSelector('.dbg-row[data-var="$order->attributes"]');
    await page.click('.dbg-row[data-var="$order->attributes"] .tree-chevron');
    await page.waitForSelector(`.dbg-row[data-var="$order->attributes['status']"]`);
    await page.locator(".dbg-vars .dbg-tree").evaluate((el) => (el.scrollTop = 0));
  },
  "debugger-stepped": async (page) => {
    await startPaused(page);
    await page.locator(".cm-content:visible").click({ position: { x: 600, y: 10 } });
    await page.keyboard.press("F8");
    await page.waitForFunction(() => document.querySelector(".cm-exec-line")?.textContent?.includes("$discount"));
    await page.keyboard.press("F8");
    await page.waitForFunction(() => document.querySelector(".cm-exec-line")?.textContent?.includes("isPayable"));
    await page.waitForSelector('.dbg-row[data-var="$discount"]');
  },
  "debugger-evaluate": async (page) => {
    await startPaused(page);
    await page.locator(".cm-content:visible").click({ position: { x: 600, y: 10 } });
    await page.keyboard.press("Alt+F8");
    await page.waitForSelector(".dbg-evaluate textarea");
    await page.fill(".dbg-evaluate textarea", "$order->attributes");
    await page.keyboard.press("Enter");
    await page.waitForSelector(`.dbg-evaluate .dbg-row[data-var="$order->attributes['total']"]`);
  },
  "debugger-breakpoints": async (page) => {
    await openController(page);
    await page.keyboard.press("Control+Shift+F8");
    await page.waitForSelector(".dbg-breakpoints");
    await page.click('.dbg-bp-item:has-text("OrderController.php:28")');
    await page.waitForSelector("#dbg-bp-cond");
  },
  "debugger-condition-popup": async (page) => {
    await openController(page);
    const dot = page.locator(".cm-bp-dot").first();
    await dot.click({ button: "right" });
    await page.waitForSelector(".dbg-bp-popup");
    await page.fill("#dbg-pop-cond", "$order->user_id === 7");
  },
  "debugger-servers": async (page) => {
    await openPhpSettings(page, "lang.php.servers");
    await page.waitForSelector('.dbg-server-row[data-server="acme-shop-test"]');
  },
  "debugger-settings": async (page) => {
    await openPhpSettings(page, "lang.php.debug");
    await page.waitForSelector('[data-setting="php.debug.port"]');
  },
  "debugger-incoming": async (page) => {
    await page.click(".dbg-listen-btn");
    await page.waitForSelector(".cm-exec-line");
    await page.evaluate(() => window.__forgeRunCommand("Stop"));
    await page.evaluate(() => window.__forgeDebugMock.connectUnknown());
    await page.waitForSelector(".dbg-incoming .dbg-candidate");
  },
  "debugger-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await startPaused(page);
    await page.click('.dbg-row[data-var="$lines"] .tree-chevron');
    await page.waitForSelector('.dbg-row[data-var="$lines->items"]');
  },
};
