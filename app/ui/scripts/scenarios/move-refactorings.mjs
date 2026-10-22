/**
 * Move (F6), Copy (F5), Safe Delete (Alt+Delete), Change Signature (Ctrl+F6) and Make
 * Static: the Move Class dialog and its preview, Usages Detected, the Change Signature and
 * Make Method Static dialogs, Move Members and the Project view's Move dialog.
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
  "move-class-dialog": async (page) => {
    await openFile(page, "Order.php");
    await caret(page, "class Order", 8);
    await page.keyboard.press("F6");
    await page.waitForSelector(".move-class-dialog");
    await page.keyboard.press("Control+A");
    await page.keyboard.type("App\\Domain\\Sales");
    await page.waitForTimeout(400);
  },
  "move-class-preview": async (page) => {
    await openFile(page, "Order.php");
    await caret(page, "class Order", 8);
    await page.keyboard.press("F6");
    await page.waitForSelector(".move-class-dialog");
    await page.keyboard.press("Control+A");
    await page.keyboard.type("App\\Domain\\Sales");
    await page.waitForTimeout(300);
    await page.click(".move-class-dialog .btn-secondary >> text=Preview");
    await page.waitForSelector(".refactoring-preview");
    await page.waitForTimeout(300);
  },
  "safe-delete-usages-detected": async (page) => {
    await openFile(page, "User.php");
    await caret(page, "function totalSpent", 12);
    await page.keyboard.press("Alt+Delete");
    await page.waitForSelector(".safe-delete-dialog");
    await page.waitForTimeout(150);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".usages-detected");
    await page.waitForTimeout(200);
  },
  "change-signature-dialog": async (page) => {
    await openFile(page, "PriceCalculator.php");
    await page.mouse.move(800, 500);
    await page.mouse.wheel(0, 300);
    await page.waitForTimeout(300);
    await caret(page, "function total(", 11);
    // Headless Chromium keeps Ctrl+F6 (focus cycling) for itself.
    await page.evaluate(() => window.__forgeRunCommand("ChangeSignature"));
    await page.waitForSelector(".change-signature-dialog");
    await page.click(".change-signature-dialog button[aria-label='Add']");
    await page.waitForTimeout(100);
    const rows = page.locator(".change-signature-dialog .signature-row:not(.head)");
    const added = rows.nth(2);
    await page.waitForTimeout(50);
    await added.locator("input[aria-label='Type']").fill("float");
    await added.locator("input[aria-label='Name']").fill("rate");
    await added.locator("input[aria-label='Default value']").fill("0.05");
    await added.locator("input[aria-label='Value at call sites']").fill("self::LOYALTY_RATE");
    await page.mouse.move(1380, 760);
    await page.waitForTimeout(200);
  },
  "safe-delete-dialog-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openFile(page, "User.php");
    await caret(page, "function totalSpent", 12);
    await page.keyboard.press("Alt+Delete");
    await page.waitForSelector(".safe-delete-dialog");
    await page.waitForTimeout(200);
  },
  "make-static-dialog": async (page) => {
    await openFile(page, "PriceCalculator.php");
    await page.mouse.move(800, 500);
    await page.mouse.wheel(0, 300);
    await page.waitForTimeout(300);
    await caret(page, "function total(", 11);
    await page.evaluate(() => window.__forgeRunCommand("MakeStatic"));
    await page.waitForSelector(".move-class-dialog");
    await page.waitForTimeout(200);
  },
  "move-members-dialog": async (page) => {
    await openFile(page, "PriceCalculator.php");
    await caret(page, "DEFAULT_TAX_RATE = 0.19", 3);
    await page.keyboard.press("F6");
    await page.waitForSelector(".move-members-dialog");
    await page.keyboard.type("App\\Services\\Checkout");
    await page.waitForTimeout(200);
  },
  "project-move-files": async (page) => {
    await page.click('.tree-row[data-path="app/Models"]');
    await page.focus(".project-tree");
    await page.keyboard.press("F6");
    await page.waitForSelector(".move-class-dialog");
    await page.keyboard.press("Control+A");
    await page.keyboard.type("app/Domain/Models");
    await page.waitForTimeout(200);
  },
};
