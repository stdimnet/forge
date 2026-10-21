/**
 * The code structure in the editor: Sticky Lines, structure breadcrumbs (bottom and top,
 * siblings menu), custom folding regions, rendered doc comments, Reader mode in a library
 * file, the Structure view with regions and the settings pages.
 */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForTimeout(300);
  await page.keyboard.press("Enter");
  await page.waitForTimeout(500);
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
}

/** Click into `text` and move the mouse away. @param {import("playwright-core").Page} page @param {string} text */
async function caretAt(page, text, nth = 0) {
  const { x, y } = await coordsOf(page, text, nth);
  await page.mouse.click(x, y);
  await page.mouse.move(1380, 860);
  await page.waitForTimeout(200);
}

/** @param {import("playwright-core").Page} page @param {number} top */
async function scrollEditor(page, top) {
  await page.evaluate((top) => {
    const s = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
    if (s) s.scrollTop = top;
  }, top);
  await page.waitForTimeout(250);
}

/** Close unrelated balloons. @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(250);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @param {import("playwright-core").Page} page */
async function priceCalculatorInLoop(page) {
  await openFile(page, "PriceCalculator");
  await scrollEditor(page, 770);
  await caretAt(page, "$line->discount;");
  await page.waitForSelector(".editor-slot:not(.hidden) .cm-sticky-line");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "sticky-lines": async (page) => {
    await priceCalculatorInLoop(page);
    await quiet(page);
  },
  "sticky-lines-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await priceCalculatorInLoop(page);
    await quiet(page);
  },
  "sticky-lines-menu": async (page) => {
    await priceCalculatorInLoop(page);
    const row = await page.$(".editor-slot:not(.hidden) .cm-sticky-line");
    const box = await row.boundingBox();
    await page.mouse.click(box.x + 300, box.y + box.height / 2, { button: "right" });
    await page.waitForSelector(".run-popup-menu");
    await quiet(page);
  },
  "breadcrumbs-siblings": async (page) => {
    await priceCalculatorInLoop(page);
    const crumb = await page.$(".editor-slot:not(.hidden) .cm-crumb.last");
    await crumb.hover();
    await page.click(".editor-slot:not(.hidden) .cm-crumb.last .cm-crumb-more");
    await page.waitForSelector(".run-popup-menu");
    await quiet(page);
  },
  "breadcrumbs-top": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.click('.set-tree-row[data-node="editor.breadcrumbs"]');
    await page.selectOption('[data-setting="editor.breadcrumbs.placement"] select', "top");
    await page.getByRole("button", { name: "OK", exact: true }).click();
    await page.waitForTimeout(400);
    await priceCalculatorInLoop(page);
    await page.waitForSelector(".editor-slot:not(.hidden) .cm-panels-top .cm-breadcrumbs-panel");
    await quiet(page);
  },
  "folding-regions": async (page) => {
    await openFile(page, "PriceCalculator");
    await page.waitForSelector(".cm-region-placeholder");
    // Fold "Configuration" with its gutter arrow.
    const { y } = await coordsOf(page, "region Configuration");
    const marker = await page.$$eval(
      ".editor-slot:not(.hidden) .cm-foldGutter .cm-gutterElement",
      (els, y) => {
        const el = els.find((e) => {
          const r = e.getBoundingClientRect();
          return r.top <= y && r.bottom >= y && e.textContent;
        });
        if (!el) return null;
        const r = el.getBoundingClientRect();
        return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
      },
      y,
    );
    if (marker) await page.mouse.click(marker.x, marker.y);
    await page.mouse.move(1380, 860);
    await quiet(page);
  },
  "rendered-doc-comments": async (page) => {
    await openFile(page, "PriceCalculator");
    // Ctrl+Alt+Q toggles the comment at the caret; Tools | Render All Doc Comments the file.
    await caretAt(page, "public function total(");
    await page.keyboard.press("Control+Alt+KeyQ");
    await page.evaluate(() => window.__forgeRunCommand("ToggleRenderedDocPresentationForAll"));
    await scrollEditor(page, 180);
    await page.waitForSelector(".cm-rendered-doc .lsp-hover-sections");
    await quiet(page);
  },
  "reader-mode": async (page) => {
    await openFile(page, "StripeGateway");
    const { x, y } = await coordsOf(page, "Throwable");
    await page.mouse.move(x, y);
    await page.keyboard.down("Control");
    await page.mouse.click(x, y);
    await page.keyboard.up("Control");
    await page.mouse.move(1380, 760);
    await page.waitForSelector(".library-banner");
    await page.waitForSelector(".cm-reader-toggle.on");
    await page.waitForSelector(".cm-rendered-doc");
    await quiet(page);
  },
  "structure-regions": async (page) => {
    await openFile(page, "PriceCalculator");
    await page.keyboard.press("Alt+Digit7");
    await page.waitForSelector(".php-glyph-region");
    await quiet(page);
  },
  "sticky-lines-settings": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.click('.set-tree-row[data-node="editor.general"]');
    await page.waitForSelector('[data-setting="editor.stickyLines.disabledLanguages"]');
    await page.evaluate(() => document.querySelector('[data-setting="editor.stickyLines.enabled"]')?.scrollIntoView());
    await quiet(page);
  },
  "breadcrumbs-settings": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.click('.set-tree-row[data-node="editor.breadcrumbs"]');
    await page.waitForSelector('[data-setting="editor.breadcrumbs.disabledLanguages"]');
    await quiet(page);
  },
  "reader-mode-settings": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.click('.set-tree-row[data-node="editor.readerMode"]');
    await page.waitForSelector('[data-setting="editor.readerMode.applyTo"]');
    await quiet(page);
  },
};
