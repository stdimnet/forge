/**
 * Go to Test (Ctrl+Shift+T): the Choose Test popup with Create New Test..., the Create New
 * PHP Test dialog (dark and light), the created test and the Create New Test dialog for a
 * TypeScript module.
 */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForFunction((name) => document.querySelector(".se-row.selected")?.textContent?.includes(name), name, { timeout: 8000 });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(500);
}

/** Put the caret into `text`. @param {import("playwright-core").Page} page */
async function caretAt(page, text) {
  await page.waitForFunction(
    (text) => document.querySelector(".editor-slot:not(.hidden) .cm-content")?.textContent?.includes(text),
    text,
    { timeout: 8000 },
  );
  const { x, y } = await page.evaluate((text) => {
    const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
    for (const line of root.querySelectorAll(".cm-line")) {
      const at = (line.textContent ?? "").indexOf(text);
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
      range.setEnd(node, Math.min(rest + 1, (node.textContent ?? "").length));
      const r = range.getBoundingClientRect();
      return { x: r.left + 1, y: r.top + r.height / 2 };
    }
    throw new Error(`text not found: ${text}`);
  }, text);
  await page.mouse.click(x, y);
  await page.mouse.move(1380, 860);
  await page.waitForTimeout(250);
}

/** Close unrelated balloons. @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(250);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
  await page.waitForTimeout(150);
}

/** Ctrl+Shift+T on PriceCalculator (no tests yet). @param {import("playwright-core").Page} page */
async function chooser(page) {
  await openFile(page, "PriceCalculator.php");
  await caretAt(page, "class PriceCalculator");
  await page.keyboard.press("Control+Shift+KeyT");
  await page.waitForSelector(".decl-chooser .decl-chooser-action");
}

/** The Create New PHP Test dialog with two methods checked. @param {import("playwright-core").Page} page */
async function phpDialog(page) {
  await chooser(page);
  await page.keyboard.press("Enter");
  await page.waitForSelector(".create-test-dialog .ct-members-row");
  const boxes = page.locator(".create-test-dialog .ct-members-row input[type=checkbox]");
  await boxes.nth(0).check();
  await boxes.nth(2).check();
  await page.locator(".create-test-dialog .ct-hooks input[type=checkbox]").first().check();
  await quiet(page);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "go-to-test-chooser": async (page) => {
    await chooser(page);
    await quiet(page);
  },
  "create-php-test-dialog": phpDialog,
  "create-php-test-dialog-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await phpDialog(page);
  },
  "create-php-test-created": async (page) => {
    await phpDialog(page);
    await page.click(".create-test-dialog .btn-primary");
    await page.waitForFunction(
      () => document.querySelector(".editor-slot:not(.hidden) .cm-content")?.textContent?.includes("testFormat"),
      null,
      { timeout: 8000 },
    );
    await page.waitForTimeout(600);
    await quiet(page);
  },
  "create-js-test-dialog": async (page) => {
    await openFile(page, "app.ts");
    await caretAt(page, "loadOrders");
    await page.keyboard.press("Control+Shift+KeyT");
    await page.waitForSelector(".decl-chooser .decl-chooser-action");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".create-test-dialog .ct-members-row");
    await page.locator(".create-test-dialog .ct-members-row input[type=checkbox]").first().check();
    await quiet(page);
  },
};
