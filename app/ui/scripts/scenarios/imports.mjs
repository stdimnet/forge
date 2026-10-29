/**
 * Optimize Imports (Ctrl+Alt+O) and Auto Import: the editor notification, the import popup
 * on an unresolved class, the "Class to Import" chooser, the Project view's "Optimize
 * Imports" dialog, Settings | Editor | General | Auto Import and the PHP code style's
 * Imports tab.
 */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForTimeout(300);
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
}

/**
 * Click into the `nth` occurrence of `text` in the visible editor, `offset` characters in.
 * @param {import("playwright-core").Page} page @param {string} text
 */
async function clickText(page, text, nth = 0, offset = 1) {
  const { x, y } = await page.evaluate(
    ([text, nth, offset]) => {
      const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
      if (!root) throw new Error("no editor");
      for (const line of root.querySelectorAll(".cm-line")) {
        const content = line.textContent ?? "";
        let at = content.indexOf(text);
        while (at >= 0) {
          if (nth-- === 0) {
            const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
            let rest = at + offset;
            let node = walker.nextNode();
            while (node && rest > (node.textContent ?? "").length) {
              rest -= (node.textContent ?? "").length;
              node = walker.nextNode();
            }
            const range = document.createRange();
            range.setStart(node, rest);
            range.setEnd(node, Math.min(rest + 1, (node.textContent ?? "").length));
            const r = range.getBoundingClientRect();
            return { x: r.left + 1, y: r.top + r.height / 2 };
          }
          at = content.indexOf(text, at + 1);
        }
      }
      throw new Error(`text not found: ${text}`);
    },
    [text, nth, offset],
  );
  await page.mouse.click(x, y);
  await page.mouse.move(1380, 760);
}

/** Close unrelated balloons (e.g. "Database found in .env"). @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** Add `line` below the `authorize` call of OrderController::show(). @param {import("playwright-core").Page} page */
async function addLine(page, line) {
  await openFile(page, "OrderController.php");
  await quiet(page);
  await clickText(page, "$this->authorize", 0, 2);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.insertText(line);
  await page.keyboard.press("Escape");
  await page.waitForTimeout(200);
}

/** @param {import("playwright-core").Page} page */
async function importPopup(page) {
  await addLine(page, "$lines = new Collection($order->lines);");
  await clickText(page, "Collection(", 0, 4);
  await page.waitForSelector(".cm-import-hint");
}

/** @param {import("playwright-core").Page} page */
async function openAutoImport(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-tree-row");
  const row = page.locator('.set-tree-row[data-node="editor.general.auto-import"]');
  if (!(await row.isVisible())) await page.click('.set-tree-row[data-node="editor.general"] .set-tree-chevron');
  await row.click();
  await page.waitForSelector('[data-setting="php.autoImport.addOnTheFly"]');
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "optimize-imports-editor": async (page) => {
    await openFile(page, "OrderController.php");
    await quiet(page);
    await clickText(page, "use ", 0, 0);
    await page.keyboard.press("Home");
    await page.keyboard.insertText("use App\\Support\\Unused;\n");
    await page.keyboard.press("Control+Alt+KeyO");
    await page.waitForSelector(".cm-lsp-hint");
  },
  "import-popup": async (page) => {
    await importPopup(page);
  },
  "import-class-chooser": async (page) => {
    await importPopup(page);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector('.decl-chooser-title:has-text("Class to Import")');
  },
  "import-popup-single": async (page) => {
    await addLine(page, "$slug = Str::slug($order->number);");
    await clickText(page, "Str::", 0, 1);
    await page.waitForSelector(".cm-import-hint");
  },
  "optimize-imports-dialog": async (page) => {
    await page.click('.tree-row[data-path="app"]', { button: "right" });
    await page.click('.context-menu-item:has-text("Optimize Imports")');
    await page.waitForSelector(".optimize-imports-dialog");
  },
  "optimize-imports-folder": async (page) => {
    await page.click('.tree-row[data-path="app"]', { button: "right" });
    await page.click('.context-menu-item:has-text("Optimize Imports")');
    await page.waitForSelector(".optimize-imports-dialog");
    await page.click('.optimize-imports-dialog button:has-text("Run")');
    await page.waitForSelector('.balloon-text:has-text("mports")');
  },
  "auto-import-settings": async (page) => {
    await openAutoImport(page);
  },
  "auto-import-settings-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openAutoImport(page);
  },
  "code-style-php-imports": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    const row = page.locator('.set-tree-row[data-node="editor.code-style.php"]');
    if (!(await row.count())) await page.click('.set-tree-row[data-node="editor.code-style"] .set-tree-chevron');
    await page.click('.set-tree-row[data-node="editor.code-style.php"]');
    await page.waitForSelector(".cs-scheme-bar");
    await page.click('.cs-tab[data-tab="imports"]');
    await page.waitForTimeout(400);
  },
};
