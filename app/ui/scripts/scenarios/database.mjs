/** Database tools: tool window tree, data sources dialog, query console and data editor. */

const DS = "shop-localhost";

/**
 * Accept the ".env" suggestion and wait until the shop schema is loaded in the tree.
 * @param {import("playwright-core").Page} page
 */
async function addShop(page) {
  await page.click('.balloon-action:has-text("Add data source")');
  await page.waitForSelector(`.db-row[data-key="table:${DS}/shop/users"]`);
}

/** @param {import("playwright-core").Page} page */
async function widenRightPanel(page, to = 470) {
  const handle = page.locator(".splitter-v").last();
  const box = await handle.boundingBox();
  if (!box) return;
  await page.mouse.move(box.x + 1, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(1440 - 40 - to, box.y + box.height / 2, { steps: 4 });
  await page.mouse.up();
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "database-tree": async (page) => {
    await addShop(page);
    await widenRightPanel(page);
    await page.click(`.db-row[data-key="table:${DS}/shop/users"] .tree-chevron`);
    await page.click(`.db-row[data-key="group:${DS}/shop/users/columns"] .tree-chevron`);
    await page.click(`.db-row[data-key="group:${DS}/shop/users/indexes"] .tree-chevron`);
    await page.click(`.db-row[data-key="folder:${DS}/shop/views"] .tree-chevron`);
    await page.click(`.db-row[data-key="folder:${DS}/shop/routines"] .tree-chevron`);
    await page.click(`.db-row[data-key="column:${DS}/shop/users/email"]`);
    await page.focus(".db-tree");
    await page.mouse.move(700, 450);
  },
  "database-data-source-dialog": async (page) => {
    await addShop(page);
    await page.click(`.db-row[data-key="ds:${DS}"]`);
    await page.evaluate(() => window.__forgeRunCommand("Database.Properties"));
    await page.waitForSelector(".dsd-dialog");
    await page.click(".dsd-test-link");
    await page.waitForSelector(".dsd-test-status.ok");
  },
  "database-console": async (page) => {
    await addShop(page);
    await page.evaluate(() => window.__forgeRunCommand("Database.OpenConsole"));
    await page.waitForSelector(".db-console .cm-content");
    await page.click(".db-console .cm-content");
    await page.keyboard.type("SELECT id, name, email, role, balance\nFROM users\nWHERE balance > 100\nORDER BY balance DESC;", { delay: 2 });
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector(".db-result .dg-row");
    await page.click(".db-console .cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\n\nSELECT number, total FROM orders WHERE orders.st", { delay: 10 });
    await page.waitForSelector(".cm-tooltip-autocomplete");
  },
  "database-grid-edit": async (page) => {
    await addShop(page);
    await page.dblclick(`.db-row[data-key="table:${DS}/shop/users"]`);
    await page.click('[data-tool-window="project"]');
    await page.waitForSelector(".db-editor .dg-row");
    // Edit two cells and mark a row for deletion.
    const cell = (row, col) => page.locator(`.db-editor .dg-row >> nth=${row}`).locator(".dg-td").nth(col);
    await cell(2, 1).dblclick();
    await page.keyboard.press("Control+a");
    await page.keyboard.type("Grace Brewster Hopper");
    await page.keyboard.press("Enter");
    await cell(4, 6).dblclick();
    await page.keyboard.press("Control+a");
    await page.keyboard.type("250.00");
    await page.keyboard.press("Enter");
    await page.locator(`.db-editor .dg-row >> nth=6`).locator(".dg-rownum").click();
    await page.keyboard.press("Control+y");
    await cell(4, 1).click();
    await page.mouse.move(900, 700);
  },
};
