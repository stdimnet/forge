/**
 * File Structure popup (Ctrl+F12), Navigation Bar (Alt+Home, dropdowns, placements) and
 * Related Symbol (Ctrl+Alt+Home).
 */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForFunction((name) => document.querySelector(".se-row.selected")?.textContent?.includes(name), name, { timeout: 8000 });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(500);
}

/** Put the caret into the `nth` occurrence of `text`. @param {import("playwright-core").Page} page */
async function caretAt(page, text, nth = 0) {
  await page.waitForFunction(
    (text) => document.querySelector(".editor-slot:not(.hidden) .cm-content")?.textContent?.includes(text),
    text,
    { timeout: 8000 },
  );
  const { x, y } = await page.evaluate(
    ([text, nth]) => {
      const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
      for (const line of root.querySelectorAll(".cm-line")) {
        const content = line.textContent ?? "";
        let at = content.indexOf(text);
        while (at >= 0) {
          if (nth-- === 0) {
            const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
            let rest = at + 1;
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
    [text, nth],
  );
  await page.mouse.click(x, y);
  await page.mouse.move(1380, 860);
  await page.waitForTimeout(250);
}

/** Close unrelated balloons. @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(250);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @param {import("playwright-core").Page} page */
async function fileStructure(page) {
  await openFile(page, "Order.php");
  await caretAt(page, "belongsTo(User");
  await page.keyboard.press("Control+F12");
  await page.waitForSelector(".fs-popup .fs-row.selected");
}

/** @param {import("playwright-core").Page} page */
async function controllerWithCaret(page) {
  await openFile(page, "ProductController");
  await caretAt(page, "products.show");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "file-structure-popup": async (page) => {
    await fileStructure(page);
    await quiet(page);
  },
  "file-structure-inherited": async (page) => {
    await fileStructure(page);
    await page.keyboard.press("Control+F12");
    await page.waitForSelector(".fs-row.inherited");
    await page.keyboard.type("sa");
    await page.waitForTimeout(300);
    await quiet(page);
  },
  "file-structure-light": async (page) => {
    await fileStructure(page);
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await quiet(page);
  },
  "navigation-bar": async (page) => {
    await controllerWithCaret(page);
    await page.waitForSelector('.navbar-top .navbar-member');
    await page.click('.navbar-top .navbar-seg[data-seg="3"]');
    await page.waitForSelector(".navbar-popup .navbar-item");
    await page.waitForTimeout(250);
    await quiet(page);
  },
  "navigation-bar-members": async (page) => {
    await controllerWithCaret(page);
    await page.click(".navbar-top .navbar-seg.last");
    await page.waitForSelector(".navbar-popup .navbar-item");
    await page.waitForTimeout(250);
    await quiet(page);
  },
  "navigation-bar-alt-home": async (page) => {
    await controllerWithCaret(page);
    await page.keyboard.press("Alt+Home");
    await page.waitForSelector(".navbar-popup .navbar-item.selected");
    await page.keyboard.type("ord");
    await page.waitForTimeout(300);
    await quiet(page);
  },
  "navigation-bar-status-bar": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("NavBarLocationBottom"));
    await page.waitForTimeout(300);
    await controllerWithCaret(page);
    await page.keyboard.press("Alt+Home");
    await page.waitForSelector(".navbar-statusBar.focused");
    await page.waitForSelector(".navbar-popup .navbar-item");
    await page.waitForTimeout(250);
    await quiet(page);
  },
  "navigation-bar-floating": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("NavBarLocationHide"));
    await page.waitForTimeout(300);
    await controllerWithCaret(page);
    await page.keyboard.press("Alt+Home");
    await page.waitForSelector(".navbar-floating .navbar-seg");
    await page.waitForSelector(".navbar-popup .navbar-item");
    await page.waitForTimeout(250);
    await quiet(page);
  },
  "navigation-bar-menu": async (page) => {
    await page.click("[data-main-menu-button]");
    await page.waitForSelector(".context-menu.main-menu-popup");
    for (const label of ["View", "Appearance", "Navigation Bar"]) {
      await page.locator(".context-menu-label", { hasText: new RegExp(`^${label}$`) }).last().hover();
      await page.waitForTimeout(150);
    }
  },
  "related-symbol": async (page) => {
    await controllerWithCaret(page);
    await page.keyboard.press("Control+Alt+Home");
    await page.waitForSelector(".decl-chooser .decl-chooser-group");
    await quiet(page);
  },
  "related-symbol-model": async (page) => {
    await openFile(page, "Order.php");
    await caretAt(page, "belongsTo(User");
    await page.keyboard.press("Control+Alt+Home");
    await page.waitForSelector(".decl-chooser .decl-chooser-row");
    await quiet(page);
  },
};
