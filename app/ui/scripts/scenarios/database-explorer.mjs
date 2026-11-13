/** Database Explorer: the full object tree (database, schemas, routines with arguments,
 * triggers, checks, sequences, types, roles), the Schemas tab with filter patterns, a
 * virtual foreign key, Quick Documentation, full-text search results and a DDL data source. */

/**
 * Add the explorer demo sources (PostgreSQL shop, DDL data source) and open the Database
 * tool window.
 * @param {import("playwright-core").Page} page
 */
async function demo(page) {
  await page.evaluate(() => window.__forgeDbMock?.addExplorerSources());
  await page.waitForSelector(".balloon .balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const close of await page.locator(".balloon .balloon-close").all()) await close.click().catch(() => undefined);
  if (!(await page.locator(".db-tree").count())) await page.click('[data-tool-window="database"]');
  await page.waitForSelector('.db-row[data-key="ds:shop-pg"]');
}

/** @param {import("playwright-core").Page} page */
async function widenRightPanel(page, to = 520) {
  const handle = page.locator(".splitter-v").last();
  const box = await handle.boundingBox();
  if (!box) return;
  await page.mouse.move(box.x + 1, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(1440 - 40 - to, box.y + box.height / 2, { steps: 4 });
  await page.mouse.up();
}

/** @param {import("playwright-core").Page} page @param {string} key */
async function expand(page, key) {
  await page.waitForSelector(`.db-row[data-key="${key}"]`);
  await page.click(`.db-row[data-key="${key}"] .tree-chevron`);
}

/** @param {import("playwright-core").Page} page */
async function openShop(page) {
  await expand(page, "ds:shop-pg");
  await page.waitForSelector('.db-row[data-key="table:shop-pg/public/orders"]');
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "database-explorer-tree": async (page) => {
    await demo(page);
    await widenRightPanel(page);
    await openShop(page);
    await expand(page, "folder:shop-pg/public/materialized views");
    await expand(page, "folder:shop-pg/public/sequences");
    await expand(page, "folder:shop-pg/public/routines");
    await expand(page, "folder:shop-pg/public/types");
    await expand(page, "table:shop-pg/public/orders");
    await expand(page, "group:shop-pg/public/orders/checks");
    await expand(page, "group:shop-pg/public/orders/triggers");
    await expand(page, "group:shop-pg/public/orders/foreign keys");
    await expand(page, "folder:shop-pg//server objects");
    await expand(page, "folder:shop-pg//roles");
    await page.click('.db-row[data-key="trigger:shop-pg/public/orders/orders_status_audit"]');
    await page.mouse.move(700, 450);
  },
  "database-quick-doc": async (page) => {
    await demo(page);
    await widenRightPanel(page);
    await openShop(page);
    await expand(page, "folder:shop-pg/public/routines");
    await page.click('.db-row[data-key^="routine:shop-pg/public/close_stale_orders"]');
    await page.focus(".db-tree");
    await page.evaluate(() => window.__forgeRunCommand("QuickJavaDoc"));
    await page.waitForSelector('[data-testid="db-quick-doc"]');
  },
  "database-schema-filter": async (page) => {
    await demo(page);
    await openShop(page);
    await page.click('.db-row[data-key="ds:shop-pg"]');
    await page.evaluate(() => window.__forgeRunCommand("Database.Properties"));
    await page.waitForSelector(".dsd-dialog");
    await page.click('[data-testid="dsd-tab-schemas"]');
    await page.waitForSelector(".dsd-schema-item");
  },
  "database-virtual-fk": async (page) => {
    await demo(page);
    await widenRightPanel(page);
    await openShop(page);
    await expand(page, "table:shop-pg/public/audit_log");
    await expand(page, "group:shop-pg/public/audit_log/columns");
    await expand(page, "group:shop-pg/public/audit_log/foreign keys");
    await page.click('.db-row[data-key="fk:shop-pg/public/audit_log/vfk_audit_log_actor_users"]');
    await expand(page, "table:shop-pg/public/order_items");
    await expand(page, "group:shop-pg/public/order_items/columns");
    await page.click('.db-row[data-key="column:shop-pg/public/order_items/product_id"]', { button: "right" });
    await page.click('.db-menu-item:has-text("Add Virtual Foreign Key")');
    await page.waitForSelector('[data-testid="db-virtual-fk-dialog"]');
    await page.selectOption('[data-testid="db-vfk-table"]', "public.products");
  },
  "database-full-text-search": async (page) => {
    await demo(page);
    await openShop(page);
    await page.click('.db-row[data-key="schema:shop-pg/public"]');
    await page.focus(".db-tree");
    await page.evaluate(() => window.__forgeRunCommand("DatabaseView.FullTextSearch"));
    await page.waitForSelector('[data-testid="fts-text"]');
    await page.fill('[data-testid="fts-text"]', "lovelace");
    await page.click('[data-testid="fts-run"]');
    await page.waitForSelector('.fts-bar-fill.done', { timeout: 15000 });
    await page.click('[data-tool-window="database"]');
    await page.mouse.move(700, 600);
  },
  "database-ddl-source": async (page) => {
    await demo(page);
    await widenRightPanel(page);
    await expand(page, "ds:catalog-ddl");
    await page.waitForSelector('.db-row[data-key="table:catalog-ddl/public/reviews"]');
    await expand(page, "folder:catalog-ddl/public/views");
    await expand(page, "folder:catalog-ddl/public/routines");
    await expand(page, "folder:catalog-ddl/public/sequences");
    await expand(page, "folder:catalog-ddl/public/types");
    await expand(page, "table:catalog-ddl/public/reviews");
    await expand(page, "group:catalog-ddl/public/reviews/columns");
    await expand(page, "group:catalog-ddl/public/reviews/triggers");
    await expand(page, "folder:catalog-ddl//problems");
    await page.dblclick('.tree-row[data-path="database"]').catch(() => undefined);
    await page.click('.db-row[data-key="column:catalog-ddl/public/reviews/verified"]');
    await page.focus(".db-tree");
    await page.evaluate(() => window.__forgeRunCommand("QuickJavaDoc"));
    await page.waitForSelector('[data-testid="db-quick-doc"]');
  },
};
