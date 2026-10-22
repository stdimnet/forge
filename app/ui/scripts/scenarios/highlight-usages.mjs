/**
 * Highlight usages like PHPStorm: the element at the caret (reads and writes, error stripe),
 * Highlight Usages in File (Ctrl+Shift+F7) with its status bar message, exit points on
 * `return`, Find Usages in File (Ctrl+F7) and the Find Usages dialog (Ctrl+Alt+Shift+F7).
 */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForTimeout(400);
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
}

/**
 * Click into the `nth` occurrence of `text` in the visible editor (one character in).
 * @param {import("playwright-core").Page} page @param {string} text
 */
async function clickText(page, text, nth = 0, offset = 1) {
  // Scroll until the occurrence is rendered (CodeMirror draws only the visible lines),
  // without smooth scrolling so the click lands where measured.
  await page.evaluate(() => document.querySelectorAll(".cm-scroller").forEach((s) => (s.style.scrollBehavior = "auto")));
  for (let i = 0; i < 20; i++) {
    const found = await page.evaluate(
      ([text, nth]) => {
        const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
        const lines = [...(root?.querySelectorAll(".cm-line") ?? [])];
        const all = lines.flatMap((l) => (l.textContent ?? "").split(text).length - 1);
        const count = all.reduce((a, b) => a + b, 0);
        if (count > nth) {
          const scroller = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
          let n = nth;
          for (const l of lines) {
            const c = (l.textContent ?? "").split(text).length - 1;
            if (n < c) {
              const r = l.getBoundingClientRect();
              const s = scroller.getBoundingClientRect();
              if (r.top >= s.top + 120 && r.bottom <= s.bottom - 60) return true;
              scroller.scrollTop += r.top - s.top - 200;
              return false;
            }
            n -= c;
          }
        }
        document.querySelector(".editor-slot:not(.hidden) .cm-scroller").scrollTop += 300;
        return false;
      },
      [text, nth],
    );
    if (found) break;
    // Smooth scrolling animates.
    await page.waitForTimeout(400);
  }
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
            while (node && rest >= (node.textContent ?? "").length) {
              rest -= (node.textContent ?? "").length;
              node = walker.nextNode();
            }
            const range = document.createRange();
            range.setStart(node, rest);
            range.setEnd(node, rest + 1);
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

/** Close unrelated balloons. @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @param {import("playwright-core").Page} page */
async function priceCalculator(page) {
  await openFile(page, "PriceCalculator");
  await quiet(page);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "highlight-usages-at-caret": async (page) => {
    await priceCalculator(page);
    await clickText(page, "$net += $price");
    await page.waitForSelector(".cm-usage-write");
    await page.waitForSelector(".cm-stripe-mark");
  },
  "highlight-usages-in-file": async (page) => {
    await priceCalculator(page);
    await clickText(page, "$price -= $price");
    await page.keyboard.press("Control+Shift+F7");
    await page.waitForSelector(".status-message");
  },
  "highlight-usages-in-file-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await priceCalculator(page);
    await clickText(page, "$price -= $price");
    await page.keyboard.press("Control+Shift+F7");
    await page.waitForSelector(".status-message");
  },
  "highlight-exit-points": async (page) => {
    await priceCalculator(page);
    await clickText(page, "throw new");
    await page.keyboard.press("Control+Shift+F7");
    await page.waitForSelector(".status-message");
  },
  "find-usages-in-file": async (page) => {
    await priceCalculator(page);
    await clickText(page, "Models\\Order;", 0, 8);
    await page.keyboard.press("Control+F7");
    await page.waitForSelector(".find-tree .find-usage");
  },
  "find-usages-dialog": async (page) => {
    await priceCalculator(page);
    await clickText(page, "Models\\Order;", 0, 8);
    await page.keyboard.press("Control+Alt+Shift+F7");
    await page.waitForSelector(".find-usages-dialog");
    await page.selectOption("#find-usages-scope", "directory");
    await page.click(".find-usages-group label:nth-of-type(2) input");
  },
};
