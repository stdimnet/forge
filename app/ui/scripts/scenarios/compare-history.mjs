/**
 * Show History (file and folder), Show History for Selection, Compare Directories, Compare
 * Files and Compare with Clipboard.
 */

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

/** The ".env" data source suggestion would cover the diff preview. */
async function closeBalloons(page) {
  await page.waitForSelector(".balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const b of await page.locator(".balloon-close").all()) await b.click().catch(() => undefined);
}

/** Taller Git tool window without balloons over it. */
async function historyLayout(page) {
  await bottomHeight(page, 430);
  await closeBalloons(page);
  await page.waitForTimeout(300);
}

/** Right-click `path` in the Project view, open the Git submenu and pick `label`. */
async function gitMenu(page, path, label) {
  await page.waitForTimeout(400);
  await page.click(`.tree-row[data-path="${path}"]`, { button: "right" });
  await page.waitForSelector(".context-menu");
  await page.hover('.context-menu > .context-menu-item:has(> .context-menu-label:text-is("Git"))');
  await page.waitForSelector(".context-menu .context-menu");
  await page.click(`.context-menu .context-menu .context-menu-item:has-text("${label}")`);
}

async function fileHistory(page, theme) {
  if (theme) await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
  await gitMenu(page, "app/Models/User.php", "Show History");
  await page.waitForSelector(".history-row");
  await page.click(".history-row:nth-child(2)");
  await page.waitForSelector(".file-history .cm-mergeView");
  await historyLayout(page);
}

/** Copies of app/Models with changes, so the folder diff has every state. */
async function makeBackup(page) {
  await page.evaluate(async () => {
    const write = window.__forgeMockWriteFile;
    await write("backup/Models/Order.php", "<?php\n\nnamespace App\\Models;\n\nclass Order\n{\n    // Restored from the 1.0 backup.\n}\n");
    await write("backup/Models/OrderStatus.php", "<?php\n\nnamespace App\\Models;\n\nenum OrderStatus: string\n{\n    case Open = 'open';\n}\n");
    await write("backup/Models/Invoice.php", "<?php\n\nnamespace App\\Models;\n\nclass Invoice\n{\n}\n");
    await write("backup/Models/Concerns/HasTotals.php", "<?php\n\nnamespace App\\Models\\Concerns;\n\ntrait HasTotals\n{\n}\n");
  });
  // Reload from Disk on the project root shows the new folder.
  await page.click(".project-tree .tree-root", { button: "right" });
  await page.click('.context-menu-item:has-text("Reload from Disk")');
  await page.waitForSelector('.tree-row[data-path="backup"]');
}

async function selectTwoFolders(page) {
  await closeBalloons(page);
  await makeBackup(page);
  await page.dblclick('.tree-row[data-path="backup"]').catch(() => {});
  if (!(await page.$('.tree-row[data-path="backup/Models"]'))) await page.click('.tree-row[data-path="backup"] .tree-chevron');
  await page.waitForSelector('.tree-row[data-path="backup/Models"]');
  await page.click('.tree-row[data-path="app/Models"] .tree-label');
  await page.click('.tree-row[data-path="backup/Models"] .tree-label', { modifiers: ["Control"] });
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "file-history": (page) => fileHistory(page),
  "file-history-light": (page) => fileHistory(page, "light"),
  "file-history-menu": async (page) => {
    await fileHistory(page);
    await page.click(".history-row:nth-child(2)", { button: "right" });
    await page.waitForSelector('.context-menu-item:has-text("Annotate Revision")');
  },
  "folder-history": async (page) => {
    await gitMenu(page, "app/Models", "Show History");
    await page.waitForSelector(".history-row");
    await page.click(".history-row:nth-child(3)");
    await page.waitForSelector(".file-history .cm-mergeView");
    await historyLayout(page);
  },
  "annotate-revision": async (page) => {
    await fileHistory(page);
    await page.click(".history-row:nth-child(3)", { button: "right" });
    await page.click('.context-menu-item:has-text("Annotate Revision")');
    await page.waitForSelector(".cm-git-blame-gutter .cm-git-blame:not(.cm-git-blame-spacer)");
    await page.waitForTimeout(400);
  },
  "selection-history": async (page) => {
    await page.waitForTimeout(400);
    const lines = page.locator(".cm-content .cm-line");
    await lines.nth(28).click();
    await page.keyboard.press("Home");
    for (let i = 0; i < 10; i++) await page.keyboard.press("Shift+ArrowDown");
    await page.evaluate(() => window.__forgeRunCommand("Vcs.ShowHistoryForBlock"));
    await page.waitForSelector(".history-row");
    await page.waitForSelector(".file-history .cm-mergeView");
    await historyLayout(page);
  },
  "compare-directories-menu": async (page) => {
    await selectTwoFolders(page);
    await page.click('.tree-row[data-path="backup/Models"] .tree-label', { button: "right" });
    await page.waitForSelector('.context-menu-item:has-text("Compare Directories")');
  },
  "compare-directories": async (page) => {
    await selectTwoFolders(page);
    await page.keyboard.press("Control+KeyD");
    await page.waitForSelector(".dir-diff-row");
    await page.click('.dir-diff-row[data-path="Order.php"]');
    await page.waitForTimeout(300);
  },
  "compare-directories-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await selectTwoFolders(page);
    await page.keyboard.press("Control+KeyD");
    await page.waitForSelector(".dir-diff-row");
    await page.click('.dir-diff-row[data-path="Order.php"]');
  },
  "compare-files": async (page) => {
    await closeBalloons(page);
    await page.click('.tree-row[data-path="app/Models/Order.php"] .tree-label');
    await page.keyboard.press("Control+KeyD");
    await page.waitForSelector(".select-path-dialog");
    // The folders of the first file are expanded already.
    await page.click('.select-path-dialog .tree-row[data-path="app/Models/User.php"]');
    await page.waitForTimeout(200);
  },
  "compare-with-clipboard": async (page) => {
    await closeBalloons(page);
    const lines = page.locator(".cm-content .cm-line");
    await lines.nth(20).click();
    await page.keyboard.press("Home");
    for (let i = 0; i < 6; i++) await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Control+KeyC");
    await lines.nth(10).click();
    await page.keyboard.press("Home");
    for (let i = 0; i < 6; i++) await page.keyboard.press("Shift+ArrowDown");
    await page.evaluate(() => void window.__forgeRunCommand("CompareClipboardWithSelection"));
    await page.waitForSelector(".diff-viewer .cm-mergeView");
    await page.waitForTimeout(300);
  },
};
