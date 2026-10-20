/** Symfony + Doctrine screenshots, on the Symfony demo project (`?project=symfony`). */

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await row.dblclick();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page */
async function caretAtLineEnd(page, text) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first();
  // Lines outside the viewport are not rendered; scroll down until it appears.
  for (let i = 0; i < 20 && !(await line.count()); i++) {
    await page.evaluate(() => {
      const s = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
      if (s) s.scrollTop += 300;
    });
    await page.waitForTimeout(80);
  }
  await line.scrollIntoViewIfNeeded();
  await line.click();
  await page.keyboard.press("End");
}

/** @param {import("playwright-core").Page} page */
async function ready(page) {
  await page.waitForSelector(".status-item", { hasText: "Symfony" }).catch(() => {});
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function showTool(page, id, tab) {
  if (!(await page.locator(`[data-tool-window-panel="${id}"]`).isVisible().catch(() => false))) {
    await page.click(`[data-tool-window="${id}"]`);
  }
  await page.waitForSelector(`[data-tool-window-panel="${id}"]`);
  // Give bottom tool windows more room, as a user would by dragging the splitter.
  const splitter = page.locator(".splitter-h").first();
  if (await splitter.isVisible().catch(() => false)) {
    const box = await splitter.boundingBox();
    if (box) {
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width / 2, box.y - 130, { steps: 5 });
      await page.mouse.up();
    }
  }
  if (tab) await page.locator(".sf-tab", { hasText: tab }).first().click();
  await page.waitForTimeout(200);
}

const scenario = (fn) => Object.assign(fn, { query: "?project=symfony" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "symfony-dql-completion": scenario(async (page) => {
    await ready(page);
    await openFile(page, "src/Repository/OrderRepository.php");
    await caretAtLineEnd(page, "WHERE o.paidAt >= :since AND o.status = :status");
    await page.keyboard.type(" AND c.");
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(300);
  }),
  "symfony-dql-querybuilder": scenario(async (page) => {
    await ready(page);
    await openFile(page, "src/Repository/OrderRepository.php");
    await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "->andWhere('o.status = :status')" }).first().click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("->andWhere('c.");
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(300);
  }),
  "symfony-twig-completion": scenario(async (page) => {
    await ready(page);
    await openFile(page, "templates/order/show.html.twig");
    await caretAtLineEnd(page, "<p class=\"total\">");
    await page.keyboard.press("Enter");
    await page.keyboard.type("<a href=\"{{ path('order_");
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(300);
  }),
  "symfony-twig-filters": scenario(async (page) => {
    await ready(page);
    await openFile(page, "templates/order/index.html.twig");
    await caretAtLineEnd(page, "<td>{{ order.customer.name }}</td>");
    await page.keyboard.press("Enter");
    await page.keyboard.type("<td>{{ order.createdAt|f");
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(300);
  }),
  "symfony-routes": scenario(async (page) => {
    await ready(page);
    await showTool(page, "symfony", "Routes");
    await page.waitForSelector(".sf-routes tbody tr");
    await page.locator('.sf-routes tr[data-route="order_show"]').click();
    await page.waitForTimeout(300);
  }),
  "symfony-services": scenario(async (page) => {
    await ready(page);
    await openFile(page, "src/Service/Checkout.php");
    await showTool(page, "symfony", "Services");
    await page.waitForSelector(".sf-services tbody tr");
    await page.locator('.sf-services tr[data-service="app.invoice_mailer"]').click();
  }),
  "symfony-console": scenario(async (page) => {
    await ready(page);
    await page.evaluate(() => window.__forgeRunCommand("Symfony.RunConsoleCommand"));
    await page.waitForSelector(".sf-console-popup .se-row");
    await page.keyboard.type("cache");
    await page.waitForTimeout(200);
  }),
  "symfony-console-run": scenario(async (page) => {
    await ready(page);
    await page.evaluate(() => window.__forgeRunCommand("Symfony.RunConsoleCommand"));
    await page.waitForSelector(".sf-console-popup .se-row");
    await page.keyboard.type("debug:router");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".sf-run-exit");
    await showTool(page, "symfony");
    await page.locator(".sf-run-tab").first().click();
  }),
  "doctrine-entities": scenario(async (page) => {
    await ready(page);
    await openFile(page, "src/Entity/Order.php");
    await showTool(page, "doctrine");
    await page.waitForSelector('.dc-entity-row[data-entity="Order"]');
    await page.locator('.dc-entity-row[data-entity="Customer"]').click();
    await page.waitForTimeout(200);
  }),
  "symfony-status-menu": scenario(async (page) => {
    await ready(page);
    await page.locator(".status-item", { hasText: "Symfony 7.2" }).click();
    await page.waitForSelector(".sf-status-menu");
  }),
  "symfony-light": scenario(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await ready(page);
    await openFile(page, "src/Repository/OrderRepository.php");
    await showTool(page, "symfony", "Routes");
    await caretAtLineEnd(page, "WHERE o.paidAt >= :since AND o.status = :status");
    await page.keyboard.type(" AND o.");
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(300);
  }),
  "symfony-settings": scenario(async (page) => {
    await ready(page);
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    const node = page.locator('.set-tree-row[data-node="lang.php.symfony"]');
    if (!(await node.isVisible().catch(() => false))) await page.click('.set-tree-row[data-node="lang.php"] .set-tree-chevron');
    await node.click();
    await page.waitForTimeout(200);
  }),
  "doctrine-settings": scenario(async (page) => {
    await ready(page);
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    const node = page.locator('.set-tree-row[data-node="lang.php.doctrine"]');
    if (!(await node.isVisible().catch(() => false))) await page.click('.set-tree-row[data-node="lang.php"] .set-tree-chevron');
    await node.click();
    await page.waitForTimeout(200);
  }),
};
