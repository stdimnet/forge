/**
 * PHP navigation like PHPStorm: Ctrl+hover links, Go to Declaration into the read-only PHP
 * stubs, Show Usages on declarations, the implemented / overridden gutter icons and the Find
 * tool window.
 */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForTimeout(300);
  await page.keyboard.press("Enter");
  await page.waitForTimeout(300);
}

/**
 * Center of the `nth` occurrence of `text` in the visible editor.
 * @param {import("playwright-core").Page} page @param {string} text
 */
async function coordsOf(page, text, nth = 0) {
  return page.evaluate(
    ([text, nth]) => {
      const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
      if (!root) throw new Error("no editor");
      for (const line of root.querySelectorAll(".cm-line")) {
        const content = line.textContent ?? "";
        let at = content.indexOf(text);
        while (at >= 0) {
          if (nth-- === 0) {
            // Find the text node and offset of `at` inside the line.
            const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
            // Point into the first word of the needle.
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
}

/** @param {import("playwright-core").Page} page @param {string} text */
async function ctrlClick(page, text, nth = 0) {
  const { x, y } = await coordsOf(page, text, nth);
  await page.mouse.move(x, y);
  await page.keyboard.down("Control");
  await page.mouse.click(x, y);
  await page.keyboard.up("Control");
  // Away from the text, so no hover documentation covers the result.
  await page.mouse.move(1380, 760);
}

/** Close unrelated balloons (e.g. "Database found in .env"). @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "php-nav-ctrl-hover": async (page) => {
    await openFile(page, "StripeGateway");
    const { x, y } = await coordsOf(page, "Throwable");
    await page.mouse.move(x, y);
    await page.keyboard.down("Control");
    await page.mouse.move(x + 2, y);
    await page.waitForSelector(".cm-lsp-link");
    await page.waitForSelector(".cm-goto-info");
    await quiet(page);
  },
  "php-nav-stub": async (page) => {
    await openFile(page, "StripeGateway");
    await ctrlClick(page, "Throwable");
    await page.waitForSelector(".library-banner");
    await quiet(page);
  },
  "php-nav-show-usages": async (page) => {
    await openFile(page, "Order.php");
    await ctrlClick(page, "Order extends");
    await page.waitForSelector(".usages-popup-row");
    await quiet(page);
  },
  "php-nav-call-sites": async (page) => {
    await openFile(page, "PaymentGateway");
    await ctrlClick(page, "charge");
    await page.waitForSelector(".usages-popup-row");
    await quiet(page);
  },
  "php-nav-gutter": async (page) => {
    await openFile(page, "PaymentGateway");
    await page.waitForSelector(".cm-line-marker.marker-impl");
    await page.locator(".cm-line-marker.marker-impl").nth(1).click();
    await page.waitForSelector(".decl-chooser-row");
    await quiet(page);
  },
  "php-nav-stubs-setting": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.click('.set-tree-row[data-node="lang.php"]');
    await page.waitForSelector(".php-settings");
    await page.evaluate(() => document.querySelector(".php-settings section:last-child")?.scrollIntoView());
    await quiet(page);
  },
  "php-nav-main-menu": async (page) => {
    await page.click("[data-main-menu-button]");
    await page.waitForSelector(".context-menu.main-menu-popup");
    await page.locator(".context-menu-label", { hasText: /^Navigate$/ }).last().hover();
    await page.waitForTimeout(150);
    await quiet(page);
  },
  "php-nav-find-window": async (page) => {
    await openFile(page, "Order.php");
    const { x, y } = await coordsOf(page, "Order extends");
    await page.mouse.click(x, y);
    await page.mouse.move(1380, 500);
    await page.keyboard.press("Alt+F7");
    await page.waitForSelector(".find-usage");
    await quiet(page);
  },
};
