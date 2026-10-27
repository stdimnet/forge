/** Schema tools: the ER diagram of a schema (Ctrl+Alt+Shift+U), Compare Structure of the
 * development and staging shop with the migration script, and Explain Plan in a console as
 * a plan tree and a flame graph. */

/**
 * Add the demo sources (PostgreSQL shop, its staging copy, DDL data source) and open the
 * Database tool window with the shop expanded.
 * @param {import("playwright-core").Page} page
 */
async function demo(page) {
  await page.evaluate(() => window.__forgeDbMock?.addSchemaToolSources());
  await page.waitForSelector(".balloon .balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const close of await page.locator(".balloon .balloon-close").all()) await close.click().catch(() => undefined);
  if (!(await page.locator(".db-tree").count())) await page.click('[data-tool-window="database"]');
  await page.waitForSelector('.db-row[data-key="ds:shop-pg"]');
  await page.click('.db-row[data-key="ds:shop-pg"] .tree-chevron');
  await page.waitForSelector('.db-row[data-key="table:shop-pg/public/orders"]');
}

/** Hide the tool windows so the editor tab gets the whole window.
 * @param {import("playwright-core").Page} page */
async function editorOnly(page) {
  await page.evaluate(() => window.__forgeRunCommand("HideAllWindows"));
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page */
async function openConsoleWith(page, sql) {
  await page.click('.db-row[data-key="ds:shop-pg"]');
  await page.evaluate(() => window.__forgeRunCommand("Database.OpenConsole"));
  await page.waitForSelector(".db-console .cm-content");
  await page.click(".db-console .cm-content");
  await page.keyboard.insertText(sql);
  await page.keyboard.press("ArrowUp");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "database-er-diagram": async (page) => {
    await demo(page);
    await page.click('.db-row[data-key="schema:shop-pg/public"]');
    await page.focus(".db-tree");
    await page.keyboard.press("Control+Alt+Shift+KeyU");
    await page.waitForSelector('[data-testid="db-diagram"] .erd-node[data-table="users"]');
    await editorOnly(page);
    await page.click('[data-testid="db-diagram"] button[title="Fit Content"]');
    await page.mouse.move(700, 880);
  },
  "database-er-diagram-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await demo(page);
    await page.click('.db-row[data-key="table:shop-pg/public/orders"]');
    await page.focus(".db-tree");
    await page.evaluate(() => window.__forgeRunCommand("ShowUmlDiagram"));
    await page.waitForSelector('[data-testid="db-diagram"] .erd-node[data-table="orders"]');
    await editorOnly(page);
    await page.click('[data-testid="db-diagram"] button[title="Fit Content"]');
    await page.click('.erd-node[data-table="orders"]');
    await page.mouse.move(700, 880);
  },
  "database-compare-structure": async (page) => {
    await demo(page);
    await page.click('.db-row[data-key="ds:shop-pg"]');
    await page.focus(".db-tree");
    await page.evaluate(() => window.__forgeRunCommand("Database.CompareStructure"));
    await page.waitForSelector('[data-testid="db-compare"]');
    await editorOnly(page);
    await page.selectOption('[data-testid="cmp-target"] select', "shop-staging");
    await page.waitForFunction(() => (document.querySelectorAll('[data-testid="cmp-target"] select').length ?? 0) > 1);
    await page.click('[data-testid="cmp-run-compare"]');
    await page.waitForSelector(".cmp-row.item");
  },
  "database-explain-plan": async (page) => {
    await demo(page);
    await openConsoleWith(page, "SELECT o.number, u.name, o.total\nFROM orders o\nJOIN users u ON u.id = o.user_id\nWHERE o.status = 'paid'\nORDER BY o.total DESC\nLIMIT 20;");
    await page.evaluate(() => window.__forgeRunCommand("Console.Jdbc.ExplainPlan"));
    await page.waitForSelector('[data-testid="plan-tree"] tbody tr');
  },
  "database-explain-flame": async (page) => {
    await demo(page);
    await openConsoleWith(page, "SELECT o.number, u.name, p.name, i.quantity\nFROM order_items i\nJOIN orders o ON o.id = i.order_id\nJOIN users u ON u.id = o.user_id\nJOIN products p ON p.id = i.product_id\nORDER BY o.total DESC;");
    await page.evaluate(() => window.__forgeRunCommand("Console.Jdbc.ExplainPlan"));
    await page.waitForSelector('[data-testid="plan-flame-toggle"]');
    await page.click('[data-testid="plan-flame-toggle"]');
    await page.waitForSelector(".plan-bar");
  },
};
