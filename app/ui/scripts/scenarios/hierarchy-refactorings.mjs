/**
 * Extract Interface / Superclass, Pull Members Up / Push Members Down and Refactor This:
 * the context-sensitive Refactor This popup on a class, a method and with speed search, the
 * Extract Interface and Extract Superclass dialogs with dependencies highlighted, the
 * Pull Members Up dialog and the preview of the result.
 */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForTimeout(300);
  await page.keyboard.press("Enter");
  await page.waitForFunction(
    (file) => document.querySelector(".editor-slot:not(.hidden) .cm-content")?.textContent?.includes(`class ${file.replace(".php", "")}`),
    name,
  );
  await page.waitForTimeout(500);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/**
 * The screen point of character `offset` of the `nth` occurrence of `text` in the editor.
 * @param {import("playwright-core").Page} page @param {string} text
 */
function pointOf(page, text, offset = 0, nth = 0) {
  return page.evaluate(
    ([text, offset, nth]) => {
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
            range.setEnd(node, rest);
            const r = range.getBoundingClientRect();
            return { x: r.left, y: r.top + r.height / 2 };
          }
          at = content.indexOf(text, at + 1);
        }
      }
      throw new Error(`text not found: ${text}`);
    },
    [text, offset, nth],
  );
}

/** Select from the start of `from` to the end of `to`. @param {import("playwright-core").Page} page */
async function select(page, from, to) {
  const a = await pointOf(page, from, 0);
  const b = await pointOf(page, to, to.length);
  await page.mouse.click(a.x + 0.5, a.y);
  await page.keyboard.down("Shift");
  await page.mouse.click(b.x - 0.5, b.y);
  await page.keyboard.up("Shift");
  await page.mouse.move(1380, 760);
}

/** @param {import("playwright-core").Page} page */
async function caret(page, text, offset) {
  const p = await pointOf(page, text, offset);
  await page.mouse.click(p.x + 0.5, p.y);
  await page.mouse.move(1380, 760);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "refactor-this-class": async (page) => {
    await openFile(page, "User.php");
    await caret(page, "class User", 8);
    await page.keyboard.press("Control+Alt+Shift+KeyT");
    await page.waitForSelector(".refactor-this .intention-row");
    await page.waitForTimeout(200);
  },
  "refactor-this-method": async (page) => {
    await openFile(page, "User.php");
    await caret(page, "function totalSpent", 12);
    await page.keyboard.press("Control+Alt+Shift+KeyT");
    await page.waitForSelector(".refactor-this .intention-row");
    await page.keyboard.type("pu");
    await page.waitForTimeout(200);
  },
  "extract-interface-dialog": async (page) => {
    await openFile(page, "User.php");
    await caret(page, "function totalSpent", 12);
    await page.evaluate(() => window.__forgeRunCommand("ExtractInterface"));
    await page.waitForSelector(".hierarchy-dialog");
    await page.keyboard.press("Control+A");
    await page.keyboard.type("HasOrders");
    await page.click(".hierarchy-dialog input[aria-label='orders()']");
    await page.click(".hierarchy-dialog .form-label.check input");
    await page.mouse.move(1380, 760);
    await page.waitForTimeout(300);
  },
  "extract-superclass-dialog": async (page) => {
    await openFile(page, "User.php");
    await caret(page, "function totalSpent", 12);
    await page.evaluate(() => window.__forgeRunCommand("ExtractSuperclass"));
    await page.waitForSelector(".hierarchy-dialog");
    await page.keyboard.press("Control+A");
    await page.keyboard.type("Customer");
    await page.click(".hierarchy-dialog input[aria-label='Make abstract totalSpent()']");
    await page.mouse.move(1380, 760);
    await page.waitForTimeout(300);
  },
  "pull-members-up-dialog": async (page) => {
    await openFile(page, "OrderController.php");
    await caret(page, "function show", 11);
    await page.evaluate(() => window.__forgeRunCommand("MembersPullUp"));
    await page.waitForSelector(".hierarchy-dialog");
    await page.mouse.move(1380, 760);
    await page.waitForTimeout(300);
  },
  "pull-members-up-preview-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openFile(page, "OrderController.php");
    await caret(page, "function show", 11);
    await page.evaluate(() => window.__forgeRunCommand("MembersPullUp"));
    await page.waitForSelector(".hierarchy-dialog");
    await page.click(".hierarchy-dialog .btn-secondary >> text=Preview");
    await page.waitForSelector(".refactoring-preview");
    await page.waitForTimeout(300);
  },
};
