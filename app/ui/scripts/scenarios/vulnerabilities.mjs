/**
 * Package Checker: the "Vulnerable Dependencies" problems in composer.json with the
 * "Upgrade to <fixed version>" quick fix, and the Vulnerable Dependencies tab of the
 * Problems tool window for the Laravel demo (composer audit) and the React storefront
 * (pnpm audit).
 */

const run = (page, id) => page.evaluate((id) => window.__forgeRunCommand(id), id);

/** @param {import("playwright-core").Page} page @param {string} path */
async function open(page, path) {
  await page.evaluate(async (path) => {
    const { openFile } = await import("/src/state/editor.ts");
    await openFile(path);
  }, path);
  await page.waitForTimeout(1500);
}

/** Put the caret after the first occurrence of `text` in the active editor. */
async function caretAfter(page, text) {
  await page.evaluate(async (text) => {
    const { visibleView } = await import("/src/editor/views.ts");
    const { activeTab } = await import("/src/state/editor.ts");
    const view = await visibleView(activeTab().path);
    view.dispatch({ selection: { anchor: view.state.doc.toString().indexOf(text) + text.length } });
    view.focus();
  }, text);
}

async function check(page) {
  await run(page, "Vulnerabilities.Check");
  await page.waitForSelector(".vuln-package", { timeout: 10000 });
  await page.waitForTimeout(800);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "vulnerabilities-quick-fix": async (page) => {
    await open(page, "composer.json");
    await check(page);
    await caretAfter(page, '"laravel/fra');
    await page.keyboard.press("Alt+Enter");
    await page.waitForTimeout(900);
  },

  "vulnerabilities-hover": async (page) => {
    await open(page, "composer.json");
    await check(page);
    const box = await page.evaluate(async () => {
      const { visibleView } = await import("/src/editor/views.ts");
      const { activeTab } = await import("/src/state/editor.ts");
      const view = await visibleView(activeTab().path);
      const at = view.state.doc.toString().indexOf("laravel/framework") + 4;
      const c = view.coordsAtPos(at);
      return { x: c.left + 2, y: (c.top + c.bottom) / 2 };
    });
    await page.mouse.move(box.x, box.y);
    await page.waitForTimeout(1200);
  },

  "vulnerabilities-view": Object.assign(
    async (page) => {
      await open(page, "package.json");
      await check(page);
    },
    { query: "?project=react" },
  ),

  "vulnerabilities-view-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await open(page, "composer.json");
    await check(page);
  },
};
