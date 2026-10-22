/**
 * Extract / Introduce and Inline: the "Expressions" popup and the in-place name of Introduce
 * Variable (Ctrl+Alt+V), the Introduce Constant dialog (Ctrl+Alt+C), the Extract Method
 * dialog (Ctrl+Alt+M) and the Inline Method dialog (Ctrl+Alt+N), in OrderController.php.
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
  "introduce-variable-expressions": async (page) => {
    await openFile(page, "OrderController.php");
    await caret(page, "$order->lines()", 10);
    await page.keyboard.press("Control+Alt+KeyV");
    await page.waitForSelector(".refactor-chooser");
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(200);
  },
  "introduce-variable-in-place": async (page) => {
    await openFile(page, "OrderController.php");
    await select(page, "$line->price", "$line->quantity");
    await page.keyboard.press("Control+Alt+KeyV");
    await page.waitForSelector(".cm-rename-primary");
    await page.waitForTimeout(250);
  },
  "introduce-constant-dialog": async (page) => {
    await openFile(page, "OrderController.php");
    await select(page, "0.1", "0.1");
    await page.keyboard.press("Control+Alt+KeyC");
    await page.waitForSelector(".introduce-dialog");
    await page.keyboard.press("Control+A");
    await page.keyboard.type("VIP_DISCOUNT");
  },
  "extract-method-dialog": async (page) => {
    await openFile(page, "OrderController.php");
    await select(page, "$lines = $order", "$line->quantity);");
    await page.keyboard.press("Control+Alt+KeyM");
    await page.waitForSelector(".extract-method-dialog");
    await page.keyboard.type("orderTotal");
    await page.waitForTimeout(150);
  },
  "extract-method-result": async (page) => {
    await openFile(page, "OrderController.php");
    await select(page, "$lines = $order", "$line->quantity);");
    await page.keyboard.press("Control+Alt+KeyM");
    await page.waitForSelector(".extract-method-dialog");
    await page.keyboard.type("orderTotal");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".extract-method-dialog", { state: "detached" });
    await page.waitForTimeout(300);
    const p = await pointOf(page, "orderTotal(", 0);
    await page.mouse.move(p.x, p.y);
    await page.mouse.wheel(0, 340);
    await page.mouse.move(1380, 760);
    await page.waitForTimeout(400);
  },
  "inline-method-dialog": async (page) => {
    await openFile(page, "OrderController.php");
    await select(page, "round($total", "2)");
    await page.keyboard.press("Control+Alt+KeyM");
    await page.waitForSelector(".extract-method-dialog");
    await page.keyboard.type("payable");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".extract-method-dialog", { state: "detached" });
    await page.waitForTimeout(400);
    await caret(page, "::payable(", 4);
    await page.keyboard.press("Control+Alt+KeyN");
    await page.waitForSelector(".inline-dialog");
  },
};
