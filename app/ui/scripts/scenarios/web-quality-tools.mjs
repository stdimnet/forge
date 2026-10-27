/**
 * Web code quality tools: Settings | Languages & Frameworks | Code Quality Tools, ESLint's
 * problems with "Fix ESLint problems" and Prettier on save (`?project=react`), Stylelint in
 * SCSS (`?project=angular`) and Biome (`?project=node`).
 */

const on = (project, fn) => Object.assign(fn, { query: `?project=${project}` });

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

/** Hover the first visible problem of `severity` so its tooltip shows. */
async function hoverProblem(page, severity) {
  const box = await page.locator(`.cm-lsp-diag-${severity}:visible`).first().boundingBox();
  if (box) await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForSelector(".cm-tooltip", { timeout: 4000 }).catch(() => {});
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "web-quality-tools-settings": on("react", async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="lang.web.quality-tools"]');
    await page.waitForSelector('.qt-tool[data-tool="eslint"]');
    await page.click('.qt-tool[data-tool="prettier"] [data-setting="web.prettier.onReformat"] input');
  }),
  "web-quality-tools-settings-light": on("node", async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="lang.web.quality-tools"]');
    await page.waitForSelector('.qt-tool[data-tool="biome"]');
    await page.click('.qt-tool[data-tool="eslint"] .qt-expand');
    await page.click('.qt-tool[data-tool="prettier"] .qt-expand');
    await page.click('.qt-tool[data-tool="biome"] button:has-text("Validate")');
    await page.waitForSelector(".qt-validation.ok");
    await page.evaluate(() => {
      const block = document.querySelector('.qt-tool[data-tool="biome"]');
      let box = block.parentElement;
      while (box && !(box.scrollHeight > box.clientHeight && getComputedStyle(box).overflowY !== "visible")) box = box.parentElement;
      if (box) box.scrollTop += block.getBoundingClientRect().bottom - box.getBoundingClientRect().bottom + 6;
    });
  }),
  "web-quality-tools-eslint": on("react", async (page) => {
    await open(page, "src/utils/cartTotals.ts");
    await page.locator(".cm-lsp-diag-error:visible").first().waitFor();
    await page.keyboard.press("Alt+Digit6");
    await page.waitForTimeout(600);
    await caretAt(page, "var total");
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector("text=Fix ESLint problems");
  }),
  "web-quality-tools-prettier-on-save": on("react", async (page) => {
    await settings(page, { "actionsOnSave.eslint": true, "actionsOnSave.prettier.enabled": true });
    await open(page, "src/utils/cartTotals.ts");
    await caretAt(page, "return Math.round");
    await page.keyboard.type(" ");
    await page.keyboard.press("Control+KeyS");
    await page.waitForSelector('.cm-line:has-text("export const formatTotal = (total: number) =>")');
    await page.waitForTimeout(1500);
    await page.keyboard.press("Alt+Digit6");
    await page.waitForTimeout(600);
  }),
  "web-quality-tools-stylelint": on("angular", async (page) => {
    await open(page, "src/app/users/user-card.component.scss");
    await page.locator(".cm-lsp-diag-error:visible").first().waitFor();
    await page.keyboard.press("Alt+Digit6");
    await page.waitForTimeout(600);
    await caretAt(page, "#ffffff");
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector("text=Fix Stylelint problems");
  }),
  "web-quality-tools-biome": on("node", async (page) => {
    await open(page, "src/orders/vouchers.ts");
    await page.locator(".cm-lsp-diag-error:visible").first().waitFor();
    await page.keyboard.press("Alt+Digit6");
    await page.waitForTimeout(600);
    await hoverProblem(page, "error");
  }),
};
