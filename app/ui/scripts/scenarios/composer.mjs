/**
 * Composer: the composer.json toolbar with outdated packages (gutter icons, weak warnings,
 * the "Update package" quick fix), completion of package names and versions, the package
 * hover, Manage Composer Dependencies, `composer install` in the Run tool window, the
 * Vulnerable Dependencies tab of `composer audit` and Settings | PHP | Composer.
 */

/** @param {import("playwright-core").Page} page @param {string} path */
async function open(page, path) {
  await page.evaluate(async (path) => {
    const { openFile } = await import("/src/state/editor.ts");
    await openFile(path);
  }, path);
  await page.waitForTimeout(1500);
}

/** Put the caret after `caret` (replacing the text first when `text` is given). */
async function caretAfter(page, caret, text) {
  await page.evaluate(
    async ([caret, text]) => {
      const { visibleView } = await import("/src/editor/views.ts");
      const { activeTab } = await import("/src/state/editor.ts");
      const view = await visibleView(activeTab().path);
      if (text !== null) view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
      const doc = view.state.doc.toString();
      view.dispatch({ selection: { anchor: doc.indexOf(caret) + caret.length } });
      view.focus();
    },
    [caret, text ?? null],
  );
}

/** Hover the middle of the first occurrence of `text`. */
async function hover(page, text) {
  const box = await page.evaluate(async (text) => {
    const { visibleView } = await import("/src/editor/views.ts");
    const { activeTab } = await import("/src/state/editor.ts");
    const view = await visibleView(activeTab().path);
    const at = view.state.doc.toString().indexOf(text) + Math.floor(text.length / 2);
    const c = view.coordsAtPos(at);
    return { x: c.left + 2, y: (c.top + c.bottom) / 2 };
  }, text);
  await page.mouse.move(box.x, box.y);
}

const run = (page, id) => page.evaluate((id) => window.__forgeRunCommand(id), id);

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "composer-editor": async (page) => {
    await open(page, "composer.json");
    await page.waitForSelector(".marker-composer-outdated", { timeout: 8000 });
    await caretAfter(page, '"laravel/framework": "^1');
    await page.keyboard.press("Alt+Enter");
    await page.waitForTimeout(900);
  },

  "composer-completion-packages": async (page) => {
    await open(page, "composer.json");
    const text = await page.evaluate(async () => {
      const { activeTab } = await import("/src/state/editor.ts");
      return activeTab().content;
    });
    await caretAfter(page, '"lara', text.replace('"guzzlehttp/guzzle": "^7.9"', '"guzzlehttp/guzzle": "^7.9",\n        "lara"'));
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 });
    await page.waitForTimeout(900);
  },

  "composer-completion-versions": async (page) => {
    await open(page, "composer.json");
    await caretAfter(page, '"doctrine/dbal": "');
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 });
    await page.waitForTimeout(900);
  },

  "composer-hover": async (page) => {
    await open(page, "composer.json");
    await hover(page, '"laravel/framework"');
    await page.waitForSelector(".cm-tooltip-hover", { timeout: 5000 });
    await page.waitForTimeout(600);
  },

  "composer-dependencies": async (page) => {
    await run(page, "Composer.AddDependency");
    await page.waitForSelector(".composer-dialog input", { timeout: 5000 });
    await page.keyboard.type("laravel");
    await page.waitForSelector(".composer-hit", { timeout: 5000 });
    await page.click('[data-composer-package="laravel/sanctum"]');
    await page.waitForTimeout(900);
  },

  "composer-install": async (page) => {
    await open(page, "composer.json");
    await page.click('[data-composer-action="Composer.Install"]');
    await page.waitForTimeout(2200);
  },

  "composer-audit": async (page) => {
    await open(page, "composer.json");
    await run(page, "Composer.Audit");
    await page.waitForSelector(".composer-audit-table", { timeout: 8000 });
    await page.waitForTimeout(1200);
  },

  "composer-settings": async (page) => {
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("lang.php.composer");
    });
    await page.waitForSelector(".composer-phar-link", { timeout: 8000 });
    await page.click(".composer-phar-link");
    await page.waitForTimeout(600);
  },

  "composer-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await open(page, "composer.json");
    await page.waitForSelector(".marker-composer-outdated", { timeout: 8000 });
    await run(page, "Composer.AddDependency");
    await page.waitForSelector(".composer-dialog input", { timeout: 5000 });
    await page.keyboard.type("doctrine");
    await page.waitForSelector(".composer-hit", { timeout: 5000 });
    await page.waitForTimeout(900);
  },
};
