/** Data editor (DataGrip): filter and sort, value editor, pending DML, FK navigation,
 * transposed view, aggregates, compare data and charts. */

const DS = "shop-localhost";

/** @param {import("playwright-core").Page} page */
async function addShop(page) {
  await page.click('.balloon-action:has-text("Add data source")');
  await page.waitForSelector(`.db-row[data-key="table:${DS}/shop/users"]`);
}

/**
 * Open a table's data editor and give it the whole window.
 * @param {import("playwright-core").Page} page
 * @param {string} table
 */
async function openTable(page, table) {
  await addShop(page);
  await page.dblclick(`.db-row[data-key="table:${DS}/shop/${table}"]`);
  await page.click('[data-tool-window="project"]');
  await page.click('[data-tool-window="database"]');
  await page.waitForSelector(`.db-editor[data-db-path="db-table://${DS}/shop/${table}"] .dg-row`);
}

/**
 * @param {import("playwright-core").Page} page
 * @param {number} row
 * @param {number} col
 */
const cell = (page, row, col) => page.locator(`.db-editor .dg-row >> nth=${row}`).locator(".dg-td").nth(col);

/** @param {import("playwright-core").Page} page */
const tool = (page, title) => page.locator(`.db-editor .db-toolbar button[title^="${title}"]`).first();

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "data-editor-filter-sort": async (page) => {
    await openTable(page, "orders");
    // Header sort: placed_at descending, then (Ctrl+Click) total.
    await page.click('.dg-th:has(.dg-th-name:text-is("status"))');
    await page.click('.dg-th:has(.dg-th-name:text-is("total"))', { modifiers: ["Control"] });
    await page.click('.dg-th:has(.dg-th-name:text-is("total"))', { modifiers: ["Control"] });
    await page.waitForSelector(".dg-sort");
    // Filter by value from a cell's context menu.
    await cell(page, 0, 3).click({ button: "right" });
    await page.click('.db-menu-item:has-text("Filter by status <>")');
    await page.waitForSelector(".db-filter-chip");
    // The WHERE field completes column names.
    await page.click('[data-testid="db-where"]');
    await page.keyboard.type("total > 100 AND cur", { delay: 20 });
    await page.waitForSelector(".db-complete-item");
  },
  "data-editor-value-json": async (page) => {
    await openTable(page, "audit_log");
    await cell(page, 1, 3).click();
    await page.keyboard.press("Shift+Enter");
    await page.waitForSelector('[data-testid="db-value-editor"] .db-value-ok');
  },
  "data-editor-pending-dml": async (page) => {
    await openTable(page, "users");
    await cell(page, 2, 1).dblclick();
    await page.keyboard.press("Control+a");
    await page.keyboard.type("Grace Brewster Hopper");
    await page.keyboard.press("Enter");
    // Set DEFAULT on a role, NULL on a token.
    await cell(page, 0, 5).click({ button: "right" });
    await page.click('.db-menu-item:has-text("Set DEFAULT")');
    await cell(page, 3, 8).click({ button: "right" });
    await page.click('.db-menu-item:has-text("Set NULL")');
    await page.locator(".db-editor .dg-row >> nth=5").locator(".dg-rownum").click();
    await page.keyboard.press("Control+y");
    await cell(page, 2, 1).click();
    await tool(page, "Preview Pending Changes").click();
    await page.waitForSelector('[data-testid="db-dml"] .db-dml-line');
    await page.mouse.move(900, 300);
  },
  "data-editor-fk-jump": async (page) => {
    await openTable(page, "orders");
    await cell(page, 4, 1).click();
    await page.keyboard.press("Control+b");
    await page.waitForSelector(`.db-editor[data-db-path="db-table://${DS}/shop/users"] .dg-row`);
    await page.waitForFunction(() => document.querySelectorAll('.db-editor[data-db-path$="/users"] .dg-row').length === 1);
    await page.locator('.db-editor[data-db-path$="/users"] .dg-row >> nth=0').locator(".dg-td").nth(1).click();
    await page.mouse.move(900, 500);
  },
  "data-editor-transposed": async (page) => {
    await openTable(page, "users");
    await cell(page, 0, 2).click();
    await tool(page, "Transpose").click();
    await page.waitForSelector('[data-testid="db-transposed"] .dgt-td');
    await page.click('[data-testid="db-transposed"] tr:nth-child(8) td:nth-child(2)');
    await tool(page, "Value Editor").click();
    await page.waitForSelector('[data-testid="db-value-editor"] img');
  },
  "data-editor-aggregates": async (page) => {
    await openTable(page, "orders");
    await cell(page, 0, 4).click();
    await cell(page, 14, 4).click({ modifiers: ["Shift"] });
    await page.waitForSelector('[data-testid="db-aggbar"]');
    await tool(page, "Aggregate View").click();
    await page.waitForSelector('[data-testid="db-aggregate-view"] .db-agg-col');
  },
  "data-editor-compare": async (page) => {
    await addShop(page);
    await page.evaluate(() => window.__forgeRunCommand("Database.OpenConsole"));
    await page.waitForSelector(".db-console .cm-content");
    await page.click('[data-tool-window="project"]');
    await page.click('[data-tool-window="database"]');
    await page.click(".db-console .cm-content");
    await page.keyboard.type(
      "SELECT * FROM products;\nUPDATE products SET price = 64.00 WHERE id = 1;\nUPDATE products SET stock = 0, is_active = FALSE WHERE id = 5;\nDELETE FROM products WHERE id = 15;\nINSERT INTO products (sku, name, price, stock, category, is_active) VALUES ('APP-1500', 'Forge Socks', 9.00, 120, 'Apparel', TRUE);",
      { delay: 1 },
    );
    await page.keyboard.press("Control+a");
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector(".db-result .dg-row");
    await page.click('.db-result-tab:has-text("products")').catch(() => {});
    await page.click('.db-result button[title="Compare With…"]');
    await page.waitForSelector('[data-testid="db-compare-dialog"]');
    await page.selectOption('[data-testid="db-compare-dialog"] select', `table:${DS}/shop/products`);
    await page.click('[data-testid="db-compare-dialog"] .db-ychip:text-is("id")');
    await page.click('[data-testid="db-compare-dialog"] .db-btn.primary');
    await page.waitForSelector('[data-testid="db-compare"] .dc-row');
  },
  "data-editor-chart": async (page) => {
    await openTable(page, "orders");
    await tool(page, "Show Chart").click();
    await page.waitForSelector('[data-testid="db-chart"] select');
    await page.selectOption('[data-testid="db-chart"] .db-chart-field:has-text("X") select', "status");
    await page.click('[data-testid="db-chart"] .db-seg-btn[title="Pie"]');
    await page.waitForSelector(".db-chart-slice");
    await page.mouse.move(900, 120);
  },
};
