/** Scratches and Consoles: New Scratch File popup, Project view node, PHP and HTTP runs. */

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

/** Close the ".env data source" balloon so it does not cover the Run tool window. */
async function dismissBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** Expand "Scratches and Consoles" > "Scratches" in the Project view (collapsing `app`). */
async function showScratches(page) {
  const app = page.locator('.tree-row[data-path="app"]');
  if ((await app.getAttribute("data-expanded")) !== null) await app.locator(".tree-chevron").click();
  await page.locator('.tree-row[data-path="scratches-and-consoles://"] .tree-chevron').click();
  await page.locator('.tree-row[data-path="scratches-and-consoles://scratches"] .tree-chevron').click();
  await page.waitForSelector('.tree-row[data-path="scratch://scratch.php"]');
}

/** Open a scratch from the Project view. */
async function openScratch(page, name) {
  await page.locator(`.tree-row[data-path="scratch://${name}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="scratch://${name}"]`);
  await page.waitForSelector(".cm-line-marker.marker-run");
  await page.waitForTimeout(150);
}

/** Run output finished: "Process finished" or the HTTP summary line. */
async function waitForOutput(page, text) {
  await page.waitForFunction((t) => [...document.querySelectorAll(".run-line")].some((l) => l.textContent?.includes(t)), text, {
    timeout: 10000,
  });
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "scratches-new-popup": async (page) => {
    await page.keyboard.press("Control+Alt+Shift+Insert");
    await page.waitForSelector(".scratch-popup-item");
    await page.keyboard.type("scr", { delay: 20 });
    await page.waitForSelector('.scratch-popup-item[data-language="javascript"]');
    await page.waitForTimeout(150);
  },
  "scratches-project-view": async (page) => {
    // A data source from the demo .env, one query console and a SQL scratch attached to it.
    await page.click('.balloon-action:has-text("Add data source")');
    await page.waitForSelector('.db-row[data-key="ds:shop-localhost"]');
    await page.click('.db-row[data-key="ds:shop-localhost"]');
    await page.evaluate(() => window.__forgeRunCommand("Database.OpenConsole"));
    await page.waitForSelector(".db-console");
    await page.evaluate(() => window.__forgeRunCommand("ActivateDatabaseToolWindow"));
    await showScratches(page);
    await page.locator('.tree-row[data-path="scratch://orders.sql"]').click({ button: "right" });
    await page.click('.run-popup-menu .run-menu-item:has-text("Attach Data Source")');
    await page.click('.scratch-popup-item:has-text("shop@localhost")');
    await page.waitForSelector('.editor-tab.active[title="scratch://orders.sql"]');
    await page.locator('.tree-row[data-path="scratches-and-consoles://consoles"] .tree-chevron').click();
    await page.locator('.tree-row[data-path="scratches-and-consoles://consoles/shop-localhost"] .tree-chevron').click();
    await openScratch(page, "scratch.php");
    await page.locator('.tree-row[data-path="scratch://rest-api.http"]').click();
    await page.waitForSelector('.editor-tab.active[title="scratch://rest-api.http"]');
    await dismissBalloons(page);
    await page.waitForTimeout(150);
  },
  "scratches-php-run": async (page) => {
    await showScratches(page);
    await openScratch(page, "scratch.php");
    await page.locator(".cm-line-marker.marker-run").first().click();
    await waitForOutput(page, "Process finished");
    await bottomHeight(page, 300);
    await dismissBalloons(page);
  },
  "scratches-http-request": async (page) => {
    await showScratches(page);
    await openScratch(page, "rest-api.http");
    // The second request: "### Show order".
    await page.locator(".cm-line-marker.marker-run").nth(1).click();
    await page.click('.run-popup-menu .run-menu-item:has-text("Run \'")');
    await waitForOutput(page, "Response code: 200");
    await bottomHeight(page, 470);
    await dismissBalloons(page);
    // Show the status line and headers, not the end of the body.
    await page.locator(".run-console").evaluate((el) => (el.scrollTop = 0));
  },
  "scratches-sql-console": async (page) => {
    // A SQL scratch attached to a data source opens as its query console; Ctrl+Enter runs.
    await page.click('.balloon-action:has-text("Add data source")');
    await page.waitForSelector('.db-row[data-key="ds:shop-localhost"]');
    await page.evaluate(() => window.__forgeRunCommand("ActivateDatabaseToolWindow"));
    await showScratches(page);
    await page.locator('.tree-row[data-path="scratch://orders.sql"]').click({ button: "right" });
    await page.click('.run-popup-menu .run-menu-item:has-text("Attach Data Source")');
    await page.click('.scratch-popup-item:has-text("shop@localhost")');
    await page.waitForSelector('.editor-tab.active[title="scratch://orders.sql"] ');
    await page.waitForSelector(".db-console .cm-content");
    await page.click(".db-console .cm-line >> nth=3");
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector(".db-result .dg-row");
    await dismissBalloons(page);
    await page.waitForTimeout(300);
  },
};
