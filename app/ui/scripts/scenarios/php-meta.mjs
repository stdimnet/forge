/**
 * PHPStorm metadata and JetBrains attributes: inspections (deprecated, expected values, pure
 * calls, PSR-4 namespace) with their quick-fixes, completion of expected arguments,
 * Settings | Directories, Mark Directory as and "External Libraries" in the Project view.
 */

/** @param {import("playwright-core").Page} page @param {string} path */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await row.click();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(1200);
}

/**
 * Click into the `nth` occurrence of `text` in the visible editor, `offset` characters in.
 * @param {import("playwright-core").Page} page @param {string} text
 */
async function clickText(page, text, offset = 1, nth = 0) {
  const { x, y } = await page.evaluate(
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
            range.setEnd(node, Math.min(rest + 1, (node.textContent ?? "").length));
            const r = range.getBoundingClientRect();
            return { x: r.left + 1, y: r.top + r.height / 2 };
          }
          at = content.indexOf(text, at + 1);
        }
      }
      throw new Error(`text not found: ${text}`);
    },
    [text, offset, nth],
  );
  await page.mouse.click(x, y);
  return { x, y };
}

/** @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @param {import("playwright-core").Page} page */
async function openSettings(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForTimeout(400);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "php-meta-inspections": async (page) => {
    await quiet(page);
    await openFile(page, "app/Http/Controllers/PaymentController.php");
    const { x, y } = await clickText(page, "payNow", 3);
    await page.mouse.move(x, y + 1);
    await page.waitForSelector(".cm-tooltip-hover, .cm-tooltip-lint", { timeout: 4000 }).catch(() => undefined);
    await page.waitForTimeout(400);
  },
  "php-meta-quickfix": async (page) => {
    await quiet(page);
    await openFile(page, "app/Http/Controllers/PaymentController.php");
    await clickText(page, "payNow", 3);
    await page.mouse.move(1380, 760);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup, .cm-intention-popup, [class*=intention]", { timeout: 4000 }).catch(() => undefined);
    await page.waitForTimeout(400);
  },
  "php-meta-completion": async (page) => {
    await quiet(page);
    await openFile(page, "app/Http/Controllers/PaymentController.php");
    // Replace 'YEN' with an expected currency: the caret right after the quote.
    await clickText(page, "'YEN'", 1);
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Delete");
    await page.keyboard.type("');");
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowLeft");
    await page.mouse.move(1380, 760);
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 4000 }).catch(() => undefined);
    await page.waitForTimeout(500);
  },
  "php-meta-namespace": async (page) => {
    await quiet(page);
    await openFile(page, "app/Support/Money.php");
    await clickText(page, "App\\Helpers", 4);
    await page.mouse.move(1380, 760);
    await page.keyboard.press("Alt+Enter");
    await page.waitForTimeout(800);
  },
  "php-directories-settings": async (page) => {
    await quiet(page);
    await openSettings(page, "directories");
    await page.waitForSelector(".php-dirs-row");
    // A folder marked by the user with its own package prefix.
    await page.fill('.php-dirs-add input[placeholder="src/Domain"]', "app/Support");
    await page.fill('.php-dirs-add input[placeholder="Package prefix"]', "App\\Helpers");
    await page.click('.php-dirs-add button[title="Add"]');
    await page.waitForSelector('.php-dirs-row[data-path="app/Support"]');
    await page.waitForTimeout(300);
  },
  "php-directories-settings-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await quiet(page);
    await openSettings(page, "directories");
    await page.waitForSelector(".php-dirs-row");
    await page.fill('.php-dirs-add input[placeholder="src/Domain"]', "app/Support");
    await page.fill('.php-dirs-add input[placeholder="Package prefix"]', "App\\Helpers");
    await page.click('.php-dirs-add button[title="Add"]');
    await page.waitForSelector('.php-dirs-row[data-path="app/Support"]');
    await page.waitForTimeout(300);
  },
  "php-meta-inspections-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await quiet(page);
    await openFile(page, "app/Http/Controllers/PaymentController.php");
    const { x, y } = await clickText(page, "'later'", 2);
    await page.mouse.move(x, y + 1);
    await page.waitForTimeout(1200);
  },
  "php-mark-directory": async (page) => {
    await quiet(page);
    await page.locator('.tree-row[data-path="app/Support"]').click({ button: "right" });
    await page.locator(".ctx-menu .ctx-item, .context-menu-item", { hasText: "Mark Directory as" }).first().hover();
    await page.waitForTimeout(500);
  },
  "php-external-libraries": async (page) => {
    await quiet(page);
    await openSettings(page, "lang.php");
    await page.fill('.php-settings input[placeholder="/usr/share/php"]', "/usr/share/php");
    await page.keyboard.press("Enter");
    await page.getByRole("button", { name: "OK", exact: true }).click();
    await page.waitForSelector('.tree-row[data-path="external-libraries://"]');
    await page.locator('.tree-row[data-path="external-libraries://"]').click();
    await page.locator('.tree-row[data-path="external-libraries:///usr/share/php"]').click();
    await page.locator('.tree-row[data-path="external-libraries:///usr/share/php/Net"]').click();
    await page.locator('.tree-row[data-path="external-libraries:///usr/share/php/Net/URL2.php"]').click();
    await page.waitForTimeout(800);
  },
};
