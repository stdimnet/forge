/** Database connectivity: SSH/SSL tab, SQL Server tree, Redis keys and values, MongoDB
 * documents, production colour with the WHERE warning, and the Sessions view. */

/**
 * Add the demo data sources of every engine and open the Database tool window.
 * @param {import("playwright-core").Page} page
 */
async function demo(page) {
  await page.evaluate(() => window.__forgeDbMock?.addDemoSources());
  // The ".env" suggestion of the demo project is not part of these shots.
  await page.waitForSelector(".balloon .balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const close of await page.locator(".balloon .balloon-close").all()) await close.click().catch(() => undefined);
  if (!(await page.locator(".db-tree").count())) await page.click('[data-tool-window="database"]');
  await page.waitForSelector('.db-row[data-key="ds:sales-prod"]');
}

/** @param {import("playwright-core").Page} page */
async function widenRightPanel(page, to = 500) {
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
  await page.click(`.db-row[data-key="${key}"] .tree-chevron`);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "database-ssh-ssl": async (page) => {
    await demo(page);
    await page.click('.db-row[data-key="ds:sales-prod"]');
    await page.evaluate(() => window.__forgeRunCommand("Database.Properties"));
    await page.waitForSelector(".dsd-dialog");
    await page.click('[data-testid="dsd-tab-ssh"]');
    await page.waitForSelector('[data-testid="dsd-ssh-page"]');
    await page.click(".dsd-test-link");
    await page.waitForSelector('[data-testid="dsd-trust-host"]');
    await page.$eval(".dsd-page", (el) => (el.scrollTop = el.scrollHeight));
  },
  "database-sqlserver-tree": async (page) => {
    await demo(page);
    await widenRightPanel(page);
    await expand(page, "ds:sales-prod");
    await page.waitForSelector('.db-row[data-key="table:sales-prod/dbo/orders"]');
    await expand(page, "table:sales-prod/dbo/orders");
    await expand(page, "group:sales-prod/dbo/orders/columns");
    await expand(page, "folder:sales-prod/dbo/routines");
    await page.click('.db-row[data-key="column:sales-prod/dbo/orders/total"]');
    await page.mouse.move(700, 450);
  },
  "database-redis": async (page) => {
    await demo(page);
    await widenRightPanel(page, 440);
    await expand(page, "ds:cache");
    await page.waitForSelector('.db-row[data-key="rkey:cache/db0/user:1"]');
    await page.dblclick('.db-row[data-key="rkey:cache/db0/leaderboard:weekly"]');
    await page.waitForSelector('[data-testid="db-redis-value"] .dg-row');
    await page.click('[data-tool-window="project"]');
    await page.click('.db-row[data-key="rkey:cache/db0/leaderboard:weekly"]');
    await page.mouse.move(700, 600);
  },
  "database-mongodb": async (page) => {
    await demo(page);
    await widenRightPanel(page, 420);
    await expand(page, "ds:catalog");
    await page.waitForSelector('.db-row[data-key="collection:catalog/shop/users"]');
    await expand(page, "collection:catalog/shop/users");
    await page.dblclick('.db-row[data-key="collection:catalog/shop/users"]');
    await page.waitForSelector(".db-result .dg-row");
    await page.click('[data-tool-window="project"]');
    await page.click('[data-testid="db-json-toggle"]');
    await page.waitForSelector('[data-testid="db-json"]');
    await page.mouse.move(700, 200);
  },
  "database-production-warning": async (page) => {
    await demo(page);
    await page.click('.db-row[data-key="ds:sales-prod"]');
    await page.evaluate(() => window.__forgeRunCommand("Database.OpenConsole"));
    await page.waitForSelector(".db-console .cm-content");
    await page.click(".db-console .cm-content");
    await page.keyboard.type("SELECT id, number, status, total\nFROM orders\nWHERE status = 'pending';\n\nDELETE FROM orders;", { delay: 2 });
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector(".db-result .dg-row");
    await page.click(".db-console .cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector("[role=alertdialog]");
  },
  "database-sessions": async (page) => {
    await demo(page);
    await page.click('.db-row[data-key="ds:analytics"]');
    await page.evaluate(() => window.__forgeRunCommand("Database.OpenConsole"));
    await page.waitForSelector(".db-console .cm-content");
    await page.click(".db-console .cm-content");
    await page.keyboard.type("SELECT status, count(*) FROM orders", { delay: 2 });
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector(".db-result .dg-row");
    await page.click('.db-row[data-key="ds:sales-prod"]');
    await page.evaluate(() => window.__forgeRunCommand("Database.OpenConsole"));
    await page.waitForSelector('.db-console[data-db-path="db-console://sales-prod/1"] .cm-content');
    await page.click('.db-console[data-db-path="db-console://sales-prod/1"] .db-chip-btn:has-text("Tx")');
    await page.click('.db-menu-item:has-text("Manual")');
    await page.click('.db-console[data-db-path="db-console://sales-prod/1"] .cm-content');
    await page.keyboard.type("UPDATE orders SET status = 'shipped' WHERE id = 7", { delay: 2 });
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector('.db-console[data-db-path="db-console://sales-prod/1"] .db-log.info:has-text("affected")');
    await widenRightPanel(page, 460);
    await page.click('[data-testid="db-sessions-toggle"]');
    await page.waitForSelector('[data-testid="db-session"]');
    await page.mouse.move(700, 600);
  },
};
