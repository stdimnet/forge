/**
 * Create from usage (Alt+Enter on an unresolved name): "Create method 'notifyCustomer'" in
 * the intention popup, the generated method with the caret in its body, and "Create class
 * 'Receipt'" opening the Create New PHP Class dialog.
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
async function createMethodPopup(page) {
  await addLine(page, "$this->notifyCustomer($order, $user);");
  await clickText(page, "notifyCustomer", 0, 3);
  await page.keyboard.press("Alt+Enter");
  await page.waitForSelector('.intention-popup .intention-row:has-text("Create method")');
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "create-from-usage-method": async (page) => {
    await createMethodPopup(page);
  },
  "create-from-usage-method-applied": async (page) => {
    await createMethodPopup(page);
    await page.keyboard.press("Enter");
    await page.waitForSelector('.cm-line:has-text("private function notifyCustomer")');
    await page.waitForTimeout(300);
  },
  "create-from-usage-method-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await createMethodPopup(page);
  },
  "create-from-usage-class": async (page) => {
    await addLine(page, "$receipt = new Receipt($order);");
    await clickText(page, "Receipt", 0, 2);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector('.intention-popup .intention-row:has-text("Create class")');
    await page.keyboard.press("Enter");
    await page.waitForSelector(".ft-dialog");
    await page.waitForTimeout(400);
  },
};
