/** SQL editor: unresolved references with a quick fix, the dialect widget, the SQL code
 * style preview, user parameters, query history, Run SQL Script and the SQL Dialects page. */

const DS = "shop-localhost";
const FILE = "database/queries/reports.sql";

/** @param {import("playwright-core").Page} page */
async function addShop(page) {
  await page.click('.balloon-action:has-text("Add data source")');
  await page.waitForSelector(`.db-row[data-key="table:${DS}/shop/users"]`);
  for (const close of await page.locator(".balloon .balloon-close").all()) await close.click().catch(() => undefined);
}

/** @param {import("playwright-core").Page} page */
async function openReports(page) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1, 1, { flash: false });
  }, FILE);
  await page.waitForSelector('[data-status="sql-dialect"]');
}

/** Pick an entry of the status bar's SQL dialect popup. @param {import("playwright-core").Page} page */
async function dialectMenu(page, label, sub) {
  await page.click('[data-status="sql-dialect"]');
  await page.waitForSelector(".context-menu");
  if (sub) {
    await page.hover(`.context-menu-label:text-is("${label}")`);
    await page.waitForTimeout(250);
    await page.click(`.context-menu-label:text-is("${sub}")`);
  } else {
    await page.click(`.context-menu-label:text-is("${label}")`);
  }
  await page.waitForTimeout(300);
}

/** The reports file as MySQL, resolved against the shop data source. @param {import("playwright-core").Page} page */
async function reportsWithShop(page) {
  await addShop(page);
  await openReports(page);
  await dialectMenu(page, "MySQL");
  await dialectMenu(page, "Resolve Against", "shop@localhost");
  await page.waitForSelector(".cm-content .cm-lsp-error, .cm-content [class*='cm-diag'], .cm-lintRange-error", { timeout: 5000 }).catch(() => undefined);
  await page.waitForTimeout(800);
}

/** @param {import("playwright-core").Page} page */
async function console(page) {
  await addShop(page);
  await page.evaluate(() => window.__forgeRunCommand("Database.OpenConsole"));
  await page.waitForSelector(".db-console .cm-content");
  await page.click(".db-console .cm-content");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "sql-editor-unresolved-quick-fix": async (page) => {
    await reportsWithShop(page);
    // Alt+Enter on UPDATE without WHERE: "Add WHERE clause".
    const line = page.locator(".cm-line", { hasText: "UPDATE orders" });
    const box = await line.boundingBox();
    if (box) await page.mouse.click(box.x + 20, box.y + box.height / 2);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup, [data-testid='intention-popup']", { timeout: 4000 }).catch(() => undefined);
    await page.waitForTimeout(300);
  },
  "sql-editor-dialect-widget": async (page) => {
    await reportsWithShop(page);
    await page.click('[data-status="sql-dialect"]');
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu-item:has-text("Resolve Against")');
    await page.waitForTimeout(300);
  },
  "sql-editor-formatter-settings": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    const row = page.locator('.set-tree-row[data-node="editor.code-style.sql"]');
    if (!(await row.count())) await page.click('.set-tree-row[data-node="editor.code-style"] .set-tree-chevron');
    await page.click('.set-tree-row[data-node="editor.code-style.sql"]');
    await page.waitForSelector(".cs-scheme-bar");
    await page.click('.cs-tab[data-tab="wrappingAndBraces"]');
    await page.waitForTimeout(300);
    await page.click('[data-option="sql.wrapping.itemsNewline"] input');
    await page.selectOption('[data-option="sql.wrapping.align"] select', "right").catch(() => undefined);
    await page.waitForTimeout(600);
  },
  "sql-editor-parameters": async (page) => {
    await console(page);
    await page.keyboard.type("SELECT id, name, email FROM users WHERE id > :min_id AND role = ?;", { delay: 2 });
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector('[data-testid="sql-params-dialog"]');
    await page.fill('[data-param=":min_id"]', "10");
    await page.fill('[data-param="?1"]', "'admin'");
  },
  "sql-editor-history": async (page) => {
    await console(page);
    for (const sql of ["SELECT count(*) FROM users;", "SELECT number, total FROM orders WHERE status = 'paid';", "SELECT name, price FROM products ORDER BY price DESC;"]) {
      await page.keyboard.press("Control+a");
      await page.keyboard.type(sql, { delay: 1 });
      await page.keyboard.press("Control+Enter");
      await page.waitForSelector(".db-result .dg-row, .db-log.info");
      await page.waitForTimeout(250);
      await page.click(".db-console .cm-content");
    }
    await page.click('[data-testid="db-history"]');
    await page.waitForSelector('[data-testid="sql-history-dialog"] .sql-history-row');
    await page.click('[data-testid="sql-history-dialog"] .sql-history-row >> nth=1');
  },
  "sql-editor-run-file": async (page) => {
    await addShop(page);
    await openReports(page);
    await page.locator(".cm-line", { hasText: "UPDATE orders" }).click({ button: "right" });
    await page.click('.context-menu-item:has-text("Run \'reports.sql\'")');
    await page.waitForSelector('[data-testid="sql-run-file-dialog"]');
    await page.click('[data-testid="sql-run-file-dialog"] .set-check input');
    await page.click('[data-testid="sql-run-file-dialog"] .btn-primary');
    await page.waitForSelector('[data-testid="sql-run-file-dialog"] .btn-primary:has-text("Close")', { timeout: 10000 });
  },
  "sql-dialects-settings": async (page) => {
    await reportsWithShop(page);
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.fill(".set-search input", "SQL Dialects");
    await page.waitForTimeout(300);
    await page.click('.set-tree-row[data-node="lang.sqlDialects"]');
    await page.waitForSelector('[data-testid="sql-dialects-page"]');
  },
};
