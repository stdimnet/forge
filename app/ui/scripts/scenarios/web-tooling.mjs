/**
 * Web tooling on the `?project=webtools` demo (an Nx workspace with a Nuxt storefront, an
 * OpenAPI document, GraphQL, Prisma, Sass and `.http` requests): File Watchers, the
 * OpenAPI preview and endpoints, GraphQL validation and completion, the Nx tool window,
 * the HTTP client's WebSocket / gRPC / OAuth requests and Import from cURL, Live Edit.
 */

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await row.click();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).dblclick();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(500);
}

/** @param {import("playwright-core").Page} page @param {string} id @param {string} waitFor */
async function settingsPage(page, id, group, waitFor) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-tree-row");
  const row = page.locator(`.set-tree-row[data-node="${id}"]`);
  if (!(await row.isVisible())) await page.click(`.set-tree-row[data-node="${group}"]`);
  await row.click();
  await page.waitForSelector(waitFor);
}

/** @param {import("playwright-core").Page} page */
async function toolWindow(page, id, waitFor) {
  if (!(await page.locator(`[data-tool-window-panel="${id}"]`).isVisible().catch(() => false))) {
    await page.click(`[data-tool-window="${id}"]`);
  }
  await page.waitForSelector(waitFor);
  await page.waitForTimeout(300);
}

const demo = (fn) => Object.assign(fn, { query: "?project=webtools" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "web-tooling-file-watchers": demo(async (page) => {
    await settingsPage(page, "tools.file-watchers", "Tools", ".fw-table tr[data-watcher]");
    await page.click('.fw-table tr[data-watcher="SCSS"]');
  }),
  "web-tooling-file-watcher-edit": demo(async (page) => {
    await settingsPage(page, "tools.file-watchers", "Tools", ".fw-table tr[data-watcher]");
    await page.dblclick('.fw-table tr[data-watcher="SCSS"]');
    await page.waitForSelector(".fw-dialog");
  }),
  "web-tooling-file-watcher-run": demo(async (page) => {
    await openFile(page, "assets/scss/_variables.scss");
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("End");
    await page.keyboard.type(" ");
    await page.keyboard.press("Control+KeyS");
    await toolWindow(page, "run", ".run-tab");
    await page.waitForTimeout(800);
  }),
  "web-tooling-openapi-preview": demo(async (page) => {
    await page.waitForSelector('[data-testid="openapi-preview"] .oa-op');
    await page.click('.oa-op[data-op="POST /products"] .oa-op-head');
    await page.waitForSelector(".oa-example");
  }),
  "web-tooling-openapi-endpoints": demo(async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("ActivateEndpointsToolWindow"));
    await page.waitForSelector(".endpoints-window");
    await page.locator(".endpoints-view-tab", { hasText: "OpenAPI" }).click();
    await page.waitForSelector('[data-testid="openapi-endpoints"] .wt-row');
  }),
  "web-tooling-nuxt-pages": demo(async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("ActivateEndpointsToolWindow"));
    await page.waitForSelector(".endpoints-window");
    await page.locator(".endpoints-view-tab", { hasText: "Nuxt" }).click();
    await page.waitForSelector('[data-testid="nuxt-view"] .wt-row');
  }),
  "web-tooling-graphql": demo(async (page) => {
    await openFile(page, "graphql/products.graphql");
    await page.waitForSelector(".cm-lintRange-error, .cm-diagnostic-error, .cm-lint-marker-error", { timeout: 8000 }).catch(() => {});
    const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "reviews(first: 3)" }).first();
    await line.click();
    await page.keyboard.press("End");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(400);
  }),
  "web-tooling-nx": demo(async (page) => {
    await toolWindow(page, "nx", '[data-testid="nx-window"] .wt-row');
    await page.click('[data-nx-project="storefront"]');
    await page.click('[data-nx-project="storefront"]');
    await page.waitForSelector('[data-nx-target="storefront:build"]');
  }),
  "web-tooling-nx-graph": demo(async (page) => {
    await toolWindow(page, "nx", '[data-testid="nx-window"] .wt-row');
    await page.locator('[data-testid="nx-window"] .wt-tabs button', { hasText: "Project Graph" }).click();
    await page.waitForSelector(".nx-node");
  }),
  "web-tooling-http-websocket": demo(async (page) => {
    await openFile(page, "requests/shop.http");
    const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "WEBSOCKET" }).first();
    await line.click();
    await page.keyboard.press("Control+Shift+F10");
    await toolWindow(page, "run", ".run-tab");
    await page.waitForTimeout(900);
  }),
  "web-tooling-http-oauth": demo(async (page) => {
    await openFile(page, "requests/shop.http");
    const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "GET https://api.acme.test" }).first();
    await line.click();
    await page.keyboard.press("Control+Shift+F10");
    await toolWindow(page, "run", ".run-tab");
    await page.waitForTimeout(900);
    await page.hover(".run-console");
    await page.mouse.wheel(0, -3000);
    await page.waitForTimeout(300);
  }),
  "web-tooling-curl-import": demo(async (page) => {
    await openFile(page, "requests/shop.http");
    await page.click('[data-testid="http-import-curl"]');
    await page.waitForSelector(".wt-curl-dialog");
    await page.fill(".wt-curl-input", "curl 'https://api.acme.test/v2/orders' \\\n  -H 'Content-Type: application/json' \\\n  -u storefront:secret \\\n  --data-raw '{\"items\":[{\"productId\":42,\"quantity\":2}]}'");
  }),
  "web-tooling-live-edit": demo(async (page) => {
    await settingsPage(page, "tools.browser-companion", "Tools", ".set-tree-row");
    await page.locator("text=Live Edit").first().scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
  }),
  "web-tooling-light": demo(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.waitForSelector('[data-testid="openapi-preview"] .oa-op');
    await page.click('.oa-op[data-op="GET /products"] .oa-op-head');
    await page.waitForSelector(".oa-table");
  }),
};
