/**
 * Spring Boot and Jakarta EE screenshots on the demo service (`?project=spring`): the
 * Endpoints tool window (Spring MVC and JAX-RS) with Open in HTTP Client, bean and injection
 * gutter icons with the injection point chooser, application.yml completion with the
 * property documentation and the unknown-property inspection, the Spring tool window
 * (Beans, Persistence), and the running application in Services (run dashboard with port,
 * profiles, actuator health, mappings and beans).
 */

const spring = (fn) => Object.assign(fn, { query: "?project=spring" });
const PKG = "src/main/java/com/acme/inventory";

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path, line = 1) {
  await page.evaluate(
    async ([p, l]) => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt(p, l, 1);
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

/** @param {import("playwright-core").Page} page */
async function command(page, id) {
  await page.evaluate((c) => window.__forgeRunCommand(c), id);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "spring-endpoints": spring(async (page) => {
    await closeBalloons(page);
    await openFile(page, `${PKG}/web/StockController.java`, 20);
    await command(page, "spring.showEndpoints");
    await page.waitForSelector(".sp-endpoints tbody tr", { timeout: 15000 });
    await bottomHeight(page, 300);
    await page.click('.sp-endpoints tr[data-endpoint="GET /api/stock/{sku}"]');
    await page.waitForTimeout(300);
    await page.click('.sp-endpoints tr[data-endpoint="GET /api/stock/{sku}"]', { button: "right" });
    await page.waitForSelector(".run-popup-menu");
    await page.waitForTimeout(300);
  }),
  "spring-http-client": spring(async (page) => {
    await closeBalloons(page);
    await command(page, "spring.showEndpoints");
    await page.waitForSelector(".sp-endpoints tbody tr", { timeout: 15000 });
    await bottomHeight(page, 260);
    const row = page.locator('.sp-endpoints tr[data-endpoint="POST /rest/warehouses/{city}/inventory"]');
    await row.hover();
    await row.locator('[data-testid="open-in-http-client"]').click();
    await page.waitForSelector('.editor-tab.active[title*=".http"]', { timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(800);
  }),
  "spring-bean-gutter": spring(async (page) => {
    await closeBalloons(page);
    await openFile(page, `${PKG}/stock/StockService.java`, 12);
    const marker = page.locator(".cm-line-marker.marker-spring-injection").nth(1);
    await marker.waitFor({ timeout: 15000 });
    await marker.click();
    await page.waitForSelector(".choose-declaration, .decl-chooser, .nav-chooser", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(500);
  }),
  "spring-yaml-completion": spring(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/main/resources/application.yml", 12);
    // Wait for the inspection (the unknown `inventory.reorder-mail`).
    await page.waitForSelector(".cm-lintRange-warning, .cm-diagnostic-warning, .cm-lint-marker-warning", { timeout: 10000 }).catch(() => {});
    const line = page.locator(".cm-line", { hasText: "show-sql: true" }).first();
    await line.click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("data");
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(700);
  }),
  "spring-tool-window": spring(async (page) => {
    await closeBalloons(page);
    await openFile(page, `${PKG}/stock/Product.java`, 8);
    await command(page, "spring.showBeans");
    await page.waitForSelector(".sp-bean", { timeout: 15000 });
    await page.waitForTimeout(400);
  }),
  "spring-run-dashboard": spring(async (page) => {
    await closeBalloons(page);
    await openFile(page, `${PKG}/InventoryApplication.java`, 8);
    await page.click('[data-tool-window="services"]');
    await page.waitForSelector('.svc-row[data-node-id="spring/app/InventoryApplication"]', { timeout: 15000 });
    await page.click('.svc-row[data-node-id="spring/app/InventoryApplication"]');
    await page.click('.svc-tool[data-action="start"]');
    // Starting shows the Run tool window; come back to Services.
    await page.waitForSelector(".run-tab, .run-session-tab, [data-tool-window-panel=\"run\"]", { timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(1500);
    await page.click('[data-tool-window="services"]');
    await page.waitForSelector('.svc-row[data-node-id="spring/app/InventoryApplication"] .svc-badge.running', { timeout: 20000 });
    await page.waitForSelector('.svc-section[data-section="Health"]', { timeout: 15000 });
    await bottomHeight(page, 420);
    await page.waitForTimeout(600);
  }),
};
