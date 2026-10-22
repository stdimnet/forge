/**
 * Type, Method and Call Hierarchy (Ctrl+H, Ctrl+Shift+H, Ctrl+Alt+H) in the Hierarchy tool
 * window on the right, with PHPStorm's toolbar, scopes, method marks and call sites.
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

/** Puts the caret into the first occurrence of `text` in the visible editor. */
async function caretAt(page, text) {
  await page.waitForFunction(
    (text) => [...document.querySelectorAll(".editor-slot:not(.hidden) .cm-line")].some((l) => (l.textContent ?? "").includes(text)),
    text,
  );
  const { x, y } = await page.evaluate((text) => {
    const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
    for (const line of root.querySelectorAll(".cm-line")) {
      const content = line.textContent ?? "";
      const at = content.indexOf(text);
      if (at < 0) continue;
      const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
      let rest = at + 1;
      let node = walker.nextNode();
      while (node && rest > (node.textContent ?? "").length) {
        rest -= (node.textContent ?? "").length;
        node = walker.nextNode();
      }
      const range = document.createRange();
      range.setStart(node, rest);
      range.setEnd(node, rest + 1);
      const r = range.getBoundingClientRect();
      return { x: r.left + 1, y: r.top + r.height / 2 };
    }
    throw new Error(`text not found: ${text}`);
  }, text);
  await page.mouse.click(x, y);
}

/** Close unrelated balloons and move the mouse off the editor. @param {import("playwright-core").Page} page */
async function settle(page) {
  await page.waitForTimeout(400);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
  await page.mouse.move(1200, 600);
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "hierarchy-type": async (page) => {
    await openFile(page, "PaymentGateway");
    await caretAt(page, "PaymentGateway");
    await page.keyboard.press("Control+KeyH");
    await page.waitForSelector(".hierarchy-row.current");
    await page.click('.hierarchy-tool[title="Subtypes Hierarchy"]');
    await page.click(".hierarchy-row:nth-child(2)");
    await settle(page);
  },
  "hierarchy-type-scope": async (page) => {
    await openFile(page, "PaymentGateway");
    await caretAt(page, "PaymentGateway");
    await page.keyboard.press("Control+KeyH");
    await page.waitForSelector(".hierarchy-row.current");
    await settle(page);
    await page.click(".hierarchy-scope");
    await page.waitForTimeout(200);
  },
  "hierarchy-method": async (page) => {
    await openFile(page, "StripeGateway");
    await caretAt(page, "charge(Order");
    await page.keyboard.press("Control+Shift+KeyH");
    await page.waitForSelector(".hierarchy-legend");
    await settle(page);
  },
  "hierarchy-call": async (page) => {
    await openFile(page, "PaymentGateway");
    await caretAt(page, "charge(Order");
    await page.keyboard.press("Control+Alt+KeyH");
    await page.waitForSelector('.hierarchy-row[data-key="root/App\\\\Services\\\\Payments\\\\PaymentService::pay()"]');
    const pay = '.hierarchy-row[data-key="root/App\\\\Services\\\\Payments\\\\PaymentService::pay()"]';
    await page.click(`${pay} .hierarchy-twisty`);
    await page.waitForTimeout(300);
    await page.click(pay);
    await settle(page);
  },
  "hierarchy-call-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openFile(page, "PaymentService");
    await caretAt(page, "pay(Order");
    await page.keyboard.press("Control+Alt+KeyH");
    await page.waitForSelector(".hierarchy-row.selected");
    await page.click('.hierarchy-tool[title="Callee Methods Hierarchy"]');
    await page.waitForTimeout(300);
    await page.click(".hierarchy-row:nth-child(2)");
    await settle(page);
  },
};
