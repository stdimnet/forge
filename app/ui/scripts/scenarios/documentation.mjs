/**
 * Documentation actions of the View menu: Quick Documentation (Ctrl+Q) and the
 * Documentation tool window, Quick Definition (Ctrl+Shift+I), Type Info (Ctrl+Shift+P),
 * Context Info (Alt+Q) and Error Description (Ctrl+F1).
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
 * Click into the `nth` occurrence of `text` in the visible editor (caret two characters in)
 * and move the mouse away so no hover popup appears.
 * @param {import("playwright-core").Page} page @param {string} text
 */
async function caretAt(page, text, nth = 0) {
  const { x, y } = await page.evaluate(
    ([text, nth]) => {
      const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
      if (!root) throw new Error("no editor");
      for (const line of root.querySelectorAll(".cm-line")) {
        const content = line.textContent ?? "";
        let at = content.indexOf(text);
        while (at >= 0) {
          if (nth-- === 0) {
            const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
            let rest = at + Math.min(2, text.length - 1);
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
  await page.waitForTimeout(150);
}

/** Close unrelated balloons. @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "quick-documentation": async (page) => {
    await caretAt(page, "hasMany(Order");
    await page.keyboard.press("Control+KeyQ");
    await page.waitForSelector(".doc-popup-doc .lsp-hover-signature");
    await quiet(page);
  },
  "documentation-tool-window": async (page) => {
    await caretAt(page, "hasMany(Order");
    await page.keyboard.press("Control+KeyQ");
    await page.waitForSelector(".doc-popup-doc");
    // A second Ctrl+Q moves the documentation to the tool window.
    await page.keyboard.press("Control+KeyQ");
    await page.waitForSelector(".doc-view-body .lsp-hover-signature");
    await quiet(page);
  },
  "quick-definition": async (page) => {
    await caretAt(page, "Order::class");
    await page.keyboard.press("Control+Shift+KeyI");
    await page.waitForSelector(".doc-popup-definition .doc-def-code");
    await quiet(page);
  },
  "type-info": async (page) => {
    await caretAt(page, "totalSpent");
    await page.keyboard.press("Control+Shift+KeyP");
    await page.waitForSelector(".doc-type");
    await quiet(page);
  },
  "context-info": async (page) => {
    // Scroll the class header out of view, like when Context Info is needed.
    await page.evaluate(() => (document.querySelector(".editor-slot:not(.hidden) .cm-scroller").scrollTop = 560));
    await page.waitForTimeout(200);
    await caretAt(page, "'email_verified_at'");
    await page.keyboard.press("Alt+KeyQ");
    await page.waitForSelector(".doc-context-row");
    await quiet(page);
  },
  "error-description": async (page) => {
    await openFile(page, "InvoiceService");
    await page.waitForSelector(".cm-lsp-diag-error");
    await caretAt(page, "markAsPaid");
    await page.keyboard.press("Control+F1");
    await page.waitForSelector(".doc-error-row");
    await quiet(page);
  },
  "quick-definition-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await caretAt(page, "Order::class");
    await page.keyboard.press("Control+Shift+KeyI");
    await page.waitForSelector(".doc-popup-definition .doc-def-code");
    await quiet(page);
  },
  "quick-documentation-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await caretAt(page, "hasMany(Order");
    await page.keyboard.press("Control+KeyQ");
    await page.waitForSelector(".doc-popup-doc .lsp-hover-signature");
    await quiet(page);
  },
};
