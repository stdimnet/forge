/**
 * Local History: Show History of a file and a folder, Show History for Selection, Recent
 * Changes (Alt+Shift+C) and one of its entries, Put Label…, the editor menu and the
 * settings page. The browser-mode demo records a few days of history at startup.
 */

/** Balloons would cover the diff. */
async function closeBalloons(page) {
  await page.waitForSelector(".balloon-close", { timeout: 2500 }).catch(() => undefined);
  for (const b of await page.locator(".balloon-close").all()) await b.click().catch(() => undefined);
}

/** Right-click `path` in the Project view, open the Local History submenu and pick `label`. */
async function localHistoryMenu(page, path, label) {
  await page.waitForTimeout(400);
  await page.click(`.tree-row[data-path="${path}"]`, { button: "right" });
  await page.waitForSelector(".context-menu");
  await page.hover('.context-menu > .context-menu-item:has(> .context-menu-label:text-is("Local History"))');
  await page.waitForSelector(".context-menu .context-menu");
  await page.click(`.context-menu .context-menu .context-menu-item:has-text("${label}")`);
}

async function fileHistory(page, theme) {
  if (theme) await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
  await localHistoryMenu(page, "app/Models/User.php", "Show History");
  await page.waitForSelector(".local-history .history-row");
  // "Refactoring: Rename method spent() to totalSpent()"
  await page.click(".local-history .history-row:nth-child(2)");
  await page.waitForSelector(".local-history .cm-mergeView");
  await closeBalloons(page);
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "local-history-file": (page) => fileHistory(page),
  "local-history-file-light": (page) => fileHistory(page, "light"),
  "local-history-menu": async (page) => {
    await page.waitForTimeout(400);
    await page.click('.tree-row[data-path="app/Models/User.php"]', { button: "right" });
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu > .context-menu-item:has(> .context-menu-label:text-is("Local History"))');
    await page.waitForSelector(".context-menu .context-menu");
    await closeBalloons(page);
  },
  "local-history-folder": async (page) => {
    await page.waitForTimeout(400);
    // The project root: every recorded change, with the label and the deleted file.
    await page.click(".project-tree .tree-root", { button: "right" });
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu > .context-menu-item:has(> .context-menu-label:text-is("Local History"))');
    await page.waitForSelector(".context-menu .context-menu");
    await page.click('.context-menu .context-menu .context-menu-item:has-text("Show History")');
    await page.waitForSelector(".local-history .history-row");
    await page.click('.local-history .history-row:has-text("Before VIP discount")');
    await page.waitForSelector(".local-history .history-files .tree-row");
    await page.click('.local-history .history-files .tree-row:has-text("LegacyInvoice.php")');
    await page.waitForSelector(".local-history .cm-mergeView");
    await closeBalloons(page);
    await page.waitForTimeout(300);
  },
  "local-history-selection": async (page) => {
    await page.waitForTimeout(400);
    await page.click('.cm-line:has-text("function totalSpent")');
    await page.evaluate(() => window.__forgeRunCommand("LocalHistory.ShowSelectionHistory"));
    await page.waitForSelector(".local-history .history-row");
    await page.waitForSelector(".local-history .cm-mergeView");
    await closeBalloons(page);
    await page.waitForTimeout(300);
  },
  "local-history-editor-menu": async (page) => {
    await page.waitForTimeout(400);
    await page.click('.cm-line:has-text("function totalSpent")', { button: "right" });
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu > .context-menu-item:has(> .context-menu-label:text-is("Local History"))');
    await page.waitForSelector(".context-menu .context-menu");
    await closeBalloons(page);
  },
  "recent-changes": async (page) => {
    await page.waitForTimeout(400);
    await closeBalloons(page);
    await page.keyboard.press("Alt+Shift+KeyC");
    await page.waitForSelector(".recent-changes-popup .recent-row");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(200);
  },
  "recent-changes-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.waitForTimeout(400);
    await closeBalloons(page);
    await page.keyboard.press("Alt+Shift+KeyC");
    await page.waitForSelector(".recent-changes-popup .recent-row");
    await page.waitForTimeout(200);
  },
  "recent-change": async (page) => {
    await page.waitForTimeout(400);
    await closeBalloons(page);
    await page.keyboard.press("Alt+Shift+KeyC");
    await page.waitForSelector(".recent-changes-popup .recent-row");
    await page.click('.recent-changes-popup .recent-row:has-text("Refactoring")');
    await page.waitForSelector(".local-history .cm-mergeView");
    await page.waitForTimeout(300);
  },
  "local-history-put-label": async (page) => {
    await page.waitForTimeout(400);
    await closeBalloons(page);
    await page.evaluate(() => void window.__forgeRunCommand("LocalHistory.PutLabel"));
    await page.waitForSelector(".popup-backdrop.modal .dialog input");
    await page.keyboard.type("Before upgrading Laravel");
  },
  "local-history-settings": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="vcs.local-history"]');
    await page.waitForSelector('[data-setting="localHistory.keepDays"]');
  },
};
