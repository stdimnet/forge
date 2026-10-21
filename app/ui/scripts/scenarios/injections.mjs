/**
 * Language injections: SQL, RegExp, JSON and HTML highlighted inside PHP strings, the
 * intentions in Show Context Actions (Alt+Enter), the language chooser of "Inject language
 * or reference", the Edit Fragment editor, Run SQL in a console (Ctrl+Enter) and
 * Settings | Editor | Language Injections.
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

/** Close unrelated balloons. @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @param {import("playwright-core").Page} page */
async function openReport(page) {
  await quiet(page);
  await openFile(page, "SalesReport");
  await page.waitForSelector(".cm-injected");
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "injections-highlighting": async (page) => {
    await openReport(page);
    await clickText(page, "SalesReport", 0, 0);
  },
  "injections-highlighting-light": async (page) => {
    await openReport(page);
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.waitForTimeout(400);
    await clickText(page, "SalesReport", 0, 0);
  },
  "injections-intentions": async (page) => {
    await openReport(page);
    await clickText(page, "JOIN users", 0, 2);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup .intention-row");
    await page.waitForTimeout(200);
  },
  "injections-inject-language": async (page) => {
    await openReport(page);
    await page.evaluate(() => {
      const scroller = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
      if (scroller) scroller.scrollTop = 420;
    });
    await page.waitForTimeout(200);
    await clickText(page, "'paid']", 0, 2);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup .intention-row");
    await page.click('.intention-row:has-text("Inject language or reference")');
    await page.waitForSelector(".injection-chooser .intention-row");
    await page.mouse.move(1380, 760);
    await page.keyboard.type("s");
    await page.waitForTimeout(250);
  },
  "injections-edit-fragment": async (page) => {
    await openReport(page);
    await clickText(page, "JOIN users", 0, 2);
    await page.evaluate(() => window.__forgeRunCommand("EditInjectedFragment"));
    await page.waitForSelector(".cm-fragment-panel .cm-content");
    await page.click(".cm-fragment-panel .cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\nOFFSET 10 -- it's synced", { delay: 10 });
    await page.waitForTimeout(300);
  },
  "injections-run-sql": async (page) => {
    await page.click('.balloon-action:has-text("Add data source")');
    await page.waitForSelector('.db-row[data-key="table:shop-localhost/shop/users"]');
    await openReport(page);
    await clickText(page, "SELECT number", 0, 3);
    await page.keyboard.press("Control+Enter");
    await page.waitForSelector(".db-result .dg-row");
    await page.waitForTimeout(300);
  },
  "injections-settings": async (page) => {
    await quiet(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="editor.injections"]');
    await page.waitForSelector('.inj-table tr[data-rule="php-html"]');
    await page.click('.inj-table tr[data-rule="php-html"] input');
    await page.click('.inj-table tr[data-rule="php-regexp"]');
    await page.waitForTimeout(200);
  },
};
