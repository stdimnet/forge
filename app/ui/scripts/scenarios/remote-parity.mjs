/**
 * Remote projects work like local ones: Laravel analysis runs on the host, data sources
 * connect through the SSH connection and Xdebug on the host reaches the IDE through a
 * forwarded port. All scenarios open the remote demo project (`?project=remote`).
 */

/** Open a project file, expanding only the folders that are still collapsed. */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await page.click(`.tree-row[data-path="${dir}"]`);
  }
  await page.click(`.tree-row[data-path="${path}"]`);
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
}

/** Make the right tool window wider. */
async function widenRightPanel(page, to = 470) {
  const handle = page.locator(".splitter-v").last();
  const box = await handle.boundingBox();
  if (!box) return;
  await page.mouse.move(box.x + 1, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(1440 - 40 - to, box.y + box.height / 2, { steps: 4 });
  await page.mouse.up();
}

/** Laravel routes of the remote project (analysed by the agent on the host). */
const routes = async (page) => {
  await page.waitForSelector("[data-testid=remote-status]");
  await page.evaluate(async () => {
    // Dev server only: give the bottom panel room for the whole table.
    const layout = await import("/src/state/layout.ts");
    layout.resizePanel("bottom", 430);
    await window.__forgeRunCommand("Laravel.ShowRoutes");
  });
  await page.waitForSelector(".routes-table tbody tr");
  await page.locator('.routes-table tr[data-route="checkout.store"]').click();
};
routes.query = "?project=remote";

/** The remote project's `.env` data source, connected through an SSH tunnel. */
const database = async (page) => {
  await page.waitForSelector("[data-testid=remote-status]");
  await page.click('.balloon-action:has-text("Add data source")');
  await page.waitForSelector('.db-row[data-key="table:shop-localhost/shop/users"]');
  await widenRightPanel(page);
  await page.click('.db-row[data-key="table:shop-localhost/shop/orders"] .tree-chevron');
  await page.click('.db-row[data-key="ds:shop-localhost"]');
  await page.mouse.move(700, 450);
};
database.query = "?project=remote";

/** Data source properties of a remote project: host and port as the host sees them. */
const dataSourceDialog = async (page) => {
  await page.waitForSelector("[data-testid=remote-status]");
  await page.click('.balloon-action:has-text("Add data source")');
  await page.waitForSelector('.db-row[data-key="table:shop-localhost/shop/users"]');
  await page.click('.db-row[data-key="ds:shop-localhost"]');
  await page.evaluate(() => window.__forgeRunCommand("Database.Properties"));
  await page.waitForSelector("[data-testid=dsd-ssh-note]");
  await page.click(".dsd-test-link");
  await page.waitForSelector(".dsd-test-status.ok");
};
dataSourceDialog.query = "?project=remote";

/** Listening for Xdebug in a remote project: the host's port 9003 is forwarded here. */
const debugListening = async (page) => {
  await page.waitForSelector("[data-testid=remote-status]");
  await openFile(page, "app/Http/Controllers/OrderController.php");
  await page.waitForSelector(".cm-bp-dot");
  await page.click(".dbg-listen-btn");
  await page.waitForSelector(".dbg-status");
  await page.waitForFunction(() => document.querySelector(".dbg-status")?.textContent?.includes("@"));
  await page.hover(".dbg-status");
};
// No request arrives, so the status bar keeps showing the listener.
debugListening.query = "?project=remote&debugIdle";

/** A request on the host hits the breakpoint: the session runs through the tunnel. */
const debugSession = async (page) => {
  await page.waitForSelector("[data-testid=remote-status]");
  await openFile(page, "app/Http/Controllers/OrderController.php");
  await page.waitForSelector(".cm-bp-dot");
  await page.click(".dbg-listen-btn");
  await page.waitForSelector(".cm-exec-line");
  await page.waitForSelector('.dbg-row[data-var="$order"]');
};
debugSession.query = "?project=remote";

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "remote-parity-laravel-routes": routes,
  "remote-parity-database": database,
  "remote-parity-data-source-dialog": dataSourceDialog,
  "remote-parity-debug-listening": debugListening,
  "remote-parity-debug-session": debugSession,
};
