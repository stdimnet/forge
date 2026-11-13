/**
 * PHP quality tools: Settings | PHP | Quality Tools, PHPStan's problems in the editor and
 * the Problems tool window, Laravel Pint on save and Rector's preview.
 */

/** @param {import("playwright-core").Page} page @param {Record<string, unknown>} values */
async function settings(page, values) {
  await page.evaluate(async (values) => {
    const { settingsApi } = await import("/src/ipc/settings.ts");
    for (const [k, v] of Object.entries(values)) await settingsApi.set(k, v, "project");
  }, values);
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page @param {string} path */
async function open(page, path) {
  await page.evaluate(async (path) => {
    const { openFile } = await import("/src/state/editor.ts");
    await openFile(path);
  }, path);
  await page.waitForTimeout(1500);
}

/** Put the caret at the start of the first occurrence of `text`. */
async function caretAt(page, text) {
  await page.evaluate(async (text) => {
    const { visibleView } = await import("/src/editor/views.ts");
    const { activeTab } = await import("/src/state/editor.ts");
    const view = await visibleView(activeTab().path);
    view.dispatch({ selection: { anchor: view.state.doc.toString().indexOf(text) }, scrollIntoView: true });
    view.focus();
  }, text);
}

const run = (page, id) => page.evaluate((id) => window.__forgeRunCommand(id), id);

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "php-quality-tools-settings": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="lang.php"] .set-tree-chevron');
    await page.click('.set-tree-row[data-node="lang.php.quality-tools"]');
    await page.waitForSelector('.qt-tool[data-tool="phpstan"]');
    await page.click('.qt-tool[data-tool="phpstan"] .qt-switch');
    await page.click('.qt-tool[data-tool="phpstan"] .qt-expand');
    await page.fill('.qt-tool[data-tool="phpstan"] [data-setting="php.tools.phpstan.level"] input', "6");
    await page.click('.qt-tool[data-tool="phpstan"] button:has-text("Validate")');
    await page.waitForSelector(".qt-validation.ok");
    // Scroll the page (not the dialog) so the whole PHPStan block shows.
    await page.evaluate(() => {
      const block = document.querySelector('.qt-tool[data-tool="phpstan"]');
      let box = block.parentElement;
      while (box && !(box.scrollHeight > box.clientHeight && getComputedStyle(box).overflowY !== "visible")) box = box.parentElement;
      if (box) box.scrollTop += block.getBoundingClientRect().bottom - box.getBoundingClientRect().bottom + 6;
    });
  },
  "php-quality-tools-list-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="lang.php"] .set-tree-chevron');
    await page.click('.set-tree-row[data-node="lang.php.quality-tools"]');
    await page.waitForSelector('.qt-tool[data-tool="phpstan"]');
    for (const t of ["phpcs", "phpstan", "pint"]) await page.click(`.qt-tool[data-tool="${t}"] .qt-switch`);
  },
  "php-quality-tools-phpstan": async (page) => {
    await settings(page, { "php.tools.phpstan.inspection": true, "php.tools.phpcs.inspection": true });
    await open(page, "app/Http/Controllers/PaymentController.php");
    await page.locator(".cm-lsp-diag-error:visible").first().waitFor();
    await caretAt(page, "'later'");
    await page.keyboard.press("Alt+Digit6");
    await page.waitForTimeout(800);
    // The tooltip of the problem under the mouse.
    const box = await page.locator(".cm-lsp-diag-error:visible").first().boundingBox();
    if (box) await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.waitForSelector(".cm-tooltip", { timeout: 4000 }).catch(() => {});
  },
  "php-quality-tools-pint-on-save": async (page) => {
    await settings(page, { "php.tools.pint.inspection": true, "actionsOnSave.pint": true });
    await open(page, "app/Models/User.php");
    await page.locator(".cm-lsp-diag-weak:visible").first().waitFor();
    await caretAt(page, "use HasFactory, Notifiable;");
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector("text=Fix with Pint");
  },
  "php-quality-tools-pint-saved": async (page) => {
    await settings(page, { "php.tools.pint.inspection": true, "actionsOnSave.pint": true });
    await open(page, "app/Models/User.php");
    await caretAt(page, "use HasFactory, Notifiable;");
    await page.keyboard.press("End");
    await page.keyboard.type(" ");
    await page.keyboard.press("Control+KeyS");
    await page.waitForSelector('.cm-line:has-text("use Notifiable;")');
    await page.waitForTimeout(800);
  },
  "php-quality-tools-rector": async (page) => {
    await settings(page, { "php.tools.rector.inspection": true });
    await open(page, "app/Services/PaymentGateway.php");
    await run(page, "PhpQualityTools.RunRector");
    await page.waitForSelector('[data-testid="rector-diff"]');
  },
};
