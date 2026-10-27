/** Database objects: Create Table with its SQL preview, Modify Table with the ALTER preview
 * and destructive-change warnings, the SQL Generator (Generate DDL), the import mapping
 * dialog and the pg_dump dialog. */

/**
 * Add the explorer demo sources (PostgreSQL shop) and open the Database tool window.
 * @param {import("playwright-core").Page} page
 */
async function demo(page) {
  await page.evaluate(() => window.__forgeDbMock?.addExplorerSources());
  await page.waitForSelector(".balloon .balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const close of await page.locator(".balloon .balloon-close").all()) await close.click().catch(() => undefined);
  if (!(await page.locator(".db-tree").count())) await page.click('[data-tool-window="database"]');
  await page.waitForSelector('.db-row[data-key="ds:shop-pg"]');
  await page.click('.db-row[data-key="ds:shop-pg"] .tree-chevron');
  await page.waitForSelector('.db-row[data-key="folder:shop-pg/public/tables"]');
}

/** @param {import("playwright-core").Page} page @param {string} key */
async function expand(page, key) {
  await page.waitForSelector(`.db-row[data-key="${key}"]`);
  await page.click(`.db-row[data-key="${key}"] .tree-chevron`);
}

/** Show the tables of the shop schema (the folder may already be open). */
/** @param {import("playwright-core").Page} page */
async function showTables(page) {
  await page.waitForSelector('.db-row[data-key="folder:shop-pg/public/tables"]');
  if (!(await page.locator('.db-row[data-key="table:shop-pg/public/orders"]').count())) await showTables(page);
  await page.waitForSelector('.db-row[data-key="table:shop-pg/public/orders"]');
}

/** @param {import("playwright-core").Page} page @param {string} key */
async function selectRow(page, key) {
  await page.waitForSelector(`.db-row[data-key="${key}"]`);
  await page.click(`.db-row[data-key="${key}"]`);
  await page.focus(".db-tree");
}

/** @param {import("playwright-core").Page} page */
async function settle(page) {
  await page.waitForTimeout(400);
  await page.mouse.move(5, 895);
}

/** @param {import("playwright-core").Page} page @param {string} label */
async function addItem(page, label) {
  await page.click('[data-testid="dbo-add"]');
  await page.click(`.db-menu-item:has-text("${label}")`);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "database-create-table": async (page) => {
    await demo(page);
    await selectRow(page, "schema:shop-pg/public");
    await page.evaluate(() => window.__forgeRunCommand("Database.NewTable"));
    await page.waitForSelector('[data-testid="dbo-table-dialog"]');
    await page.click('[data-testid="dbo-node-table-0"]');
    await page.fill('[data-testid="dbo-table-name"]', "invoices");
    await addItem(page, "Column");
    await page.fill('[data-testid="dbo-col-name"]', "order_id");
    await page.fill('[data-testid="dbo-col-type"]', "bigint");
    await page.click('[data-testid="dbo-props"] label:has-text("Not null") input');
    await addItem(page, "Column");
    await page.fill('[data-testid="dbo-col-name"]', "total");
    await page.fill('[data-testid="dbo-col-type"]', "numeric(12,2)");
    await page.fill('[data-testid="dbo-props"] input[placeholder="no default"]', "0");
    await addItem(page, "Column");
    await page.fill('[data-testid="dbo-col-name"]', "issued_at");
    await page.fill('[data-testid="dbo-col-type"]', "timestamp with time zone");
    await addItem(page, "Foreign Key");
    await page.selectOption('[data-testid="dbo-fk-target"]', "public.orders");
    await page.click('[data-testid="dbo-props"] .dbo-picker >> nth=0 >> label:has-text("order_id") input');
    await page.selectOption('[data-testid="dbo-props"] select >> nth=2', "CASCADE");
    await addItem(page, "Index");
    await page.waitForFunction(() => document.querySelector('[data-testid="dbo-table-sql"]')?.textContent?.includes("CREATE INDEX"));
    await settle(page);
  },
  "database-modify-table": async (page) => {
    await demo(page);
    await showTables(page);
    await selectRow(page, "table:shop-pg/public/products");
    await page.evaluate(() => window.__forgeRunCommand("Database.ModifyTable"));
    await page.waitForSelector('[data-testid="dbo-node-columns-1"]');
    // Rename a column, change a type, drop one and add one.
    await page.click('[data-testid="dbo-node-columns-1"]');
    await page.fill('[data-testid="dbo-col-name"]', "title");
    await page.click('[data-testid="dbo-node-columns-2"]');
    const type = await page.inputValue('[data-testid="dbo-col-type"]');
    await page.fill('[data-testid="dbo-col-type"]', type.startsWith("numeric") ? "numeric(12,2)" : "text");
    const last = await page.locator('[data-testid^="dbo-node-columns-"]').count();
    await page.click(`[data-testid="dbo-node-columns-${last - 1}"]`);
    await page.click('.dbo-toolbar button[title="Remove"]');
    await addItem(page, "Column");
    await page.fill('[data-testid="dbo-col-name"]', "weight_kg");
    await page.fill('[data-testid="dbo-col-type"]', "numeric(8,3)");
    await page.waitForSelector('[data-testid="dbo-warnings"]');
    await settle(page);
  },
  "database-generate-ddl": async (page) => {
    await demo(page);
    await showTables(page);
    await page.click('.db-row[data-key="table:shop-pg/public/users"]');
    await page.click('.db-row[data-key="table:shop-pg/public/orders"]', { modifiers: ["Control"] });
    await page.click('.db-row[data-key="table:shop-pg/public/order_items"]', { modifiers: ["Control"] });
    await page.click('.db-row[data-key="table:shop-pg/public/order_items"]', { button: "right" });
    await page.click('.db-menu-item:has-text("SQL Scripts")');
    await page.click('.db-menu-item:has-text("SQL Generator")');
    await page.waitForSelector('[data-testid="dbo-ddl-dialog"]');
    await page.click('[data-testid="dbo-ddl-includeDrops"]');
    await page.waitForFunction(() => document.querySelector('[data-testid="dbo-ddl-sql"]')?.textContent?.includes("DROP TABLE"));
    await settle(page);
  },
  "database-objects-menu": async (page) => {
    await demo(page);
    await showTables(page);
    await page.click('.db-row[data-key="table:shop-pg/public/orders"]', { button: "right" });
    await page.hover('.db-menu-item:has-text("SQL Scripts")');
    await page.waitForSelector(".db-submenu");
    await page.waitForTimeout(200);
  },
  "database-import": async (page) => {
    await demo(page);
    await page.click('.db-row[data-key="schema:shop-pg/public"]', { button: "right" });
    await page.click('.db-menu-item:has-text("Import/Export")');
    await page.click('.db-menu-item:has-text("Import Data from File")');
    await page.waitForSelector('[data-testid="dbo-import-dialog"]');
    const csv = [
      "Customer ID;Name;E-Mail;Joined;Balance;VIP",
      "1;Ada Lovelace;ada@example.com;2024-03-01;1250.50;yes",
      '2;"Hopper; Grace";grace@example.com;2024-03-02;0;no',
      "3;Alan Turing;;2024-03-05;-7.25;no",
      "4;Katherine Johnson;kj@example.com;2024-04-11;310;yes",
      "5;Margaret Hamilton;mh@example.com;2024-05-20;87.10;no",
    ].join("\n");
    await page.setInputFiles('[data-testid="dbo-import-file"]', { name: "customers.csv", mimeType: "text/csv", buffer: Buffer.from(csv) });
    await page.waitForSelector('[data-testid="dbo-import-preview"] tbody tr');
    await page.waitForSelector('[data-testid="dbo-import-create"]');
    await settle(page);
  },
  "database-dump": async (page) => {
    await demo(page);
    await selectRow(page, "schema:shop-pg/public");
    await page.evaluate(() => window.__forgeRunCommand("Database.DumpData"));
    await page.waitForSelector('[data-testid="dbo-dump-dialog"]');
    await page.fill('[data-testid="dbo-dump-file"]', "/home/dev/backups/shop-2024-06-01.dump");
    await page.waitForFunction(() => document.querySelector('[data-testid="dbo-dump-command"]')?.textContent?.includes("pg_dump"));
    await settle(page);
  },
};
