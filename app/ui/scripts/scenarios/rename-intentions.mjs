/**
 * Rename (Shift+F6) in place, the Refactoring Preview with conflicts, the Intelephense
 * Premium hint, Show Context Actions (Alt+Enter) with quick fixes and intentions, and the
 * light bulb.
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

/** @param {import("playwright-core").Page} page */
async function premium(page) {
  await page.evaluate(() => window.__forgeMockLicence?.("DEMO-LICENCE-1234"));
}

/** Start renaming `Order` in User.php and type the new name. @param {import("playwright-core").Page} page */
async function renameOrder(page, name) {
  await quiet(page);
  await clickText(page, "Order::class", 0, 2);
  await page.keyboard.press("Shift+F6");
  await page.waitForSelector(".cm-rename-primary");
  await page.keyboard.type(name, { delay: 30 });
  await page.waitForTimeout(150);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "rename-in-place": async (page) => {
    await renameOrder(page, "Purchase");
    await page.waitForSelector(".cm-rename-hint");
  },
  "rename-premium-hint": async (page) => {
    await renameOrder(page, "Purchase");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".balloon", { hasText: "Renamed with the PHP index" });
    await page.waitForTimeout(400);
  },
  "rename-refactoring-preview": async (page) => {
    await renameOrder(page, "OrderStatus");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".refactoring-preview .preview-conflict");
  },
  "rename-conflicts-in-find": async (page) => {
    await renameOrder(page, "OrderStatus");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".refactoring-preview .preview-conflict");
    await page.click('.refactoring-preview button:has-text("Show in View")');
    await page.waitForSelector(".find-window .find-usage");
  },
  "rename-dialog": async (page) => {
    await renameOrder(page, "Purchase");
    await page.keyboard.press("Shift+F6");
    await page.waitForSelector(".rename-dialog");
  },
  "context-actions": async (page) => {
    await premium(page);
    await openFile(page, "InvoiceService.php");
    await quiet(page);
    await clickText(page, "$checkout", 0, 3);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup .intention-row");
    await page.keyboard.press("ArrowRight");
    await page.waitForSelector(".intention-submenu");
  },
  "context-actions-intentions": async (page) => {
    await openFile(page, "Checkout.php");
    await quiet(page);
    await clickText(page, "$currency;", 0, 3);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup .intention-row");
  },
  "light-bulb": async (page) => {
    await premium(page);
    await openFile(page, "InvoiceService.php");
    await quiet(page);
    await clickText(page, "utf8_encode", 0, 3);
    await page.waitForSelector(".cm-intention-bulb.fix", { state: "visible" });
  },
};
