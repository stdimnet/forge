/**
 * WebForge screenshots on the web demo projects: `?project=react` (storefront-ui, pnpm
 * workspace), `?project=vue` (vue-dashboard, yarn) and `?project=angular` (admin-portal).
 */

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await row.click();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).dblclick();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** Put the caret `offset` characters after `needle` on the first line that contains it. */
async function caretAt(page, needle, offset = 0) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: needle }).first();
  await line.scrollIntoViewIfNeeded();
  await line.click();
  const text = await line.innerText();
  await page.keyboard.press("Home");
  // Home stops at the indentation; go to column 0 first.
  const indent = text.length - text.trimStart().length;
  const column = text.indexOf(needle) + offset;
  for (let i = 0; i < Math.abs(column - indent); i++) await page.keyboard.press(column > indent ? "ArrowRight" : "ArrowLeft");
}

/** @param {import("playwright-core").Page} page */
async function showNpm(page) {
  if (!(await page.locator('[data-tool-window-panel="npm"]').isVisible().catch(() => false))) {
    await page.click('[data-tool-window="npm"]');
  }
  await page.waitForSelector(".npm-script");
}

/** @param {import("playwright-core").Page} page */
async function ready(page, label) {
  await page.waitForSelector(".status-item", { hasText: label }).catch(() => {});
  await page.waitForTimeout(400);
}

const on = (project, fn) => Object.assign(fn, { query: `?project=${project}` });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "webforge-npm-tool-window": on("react", async (page) => {
    await ready(page, "React");
    await openFile(page, "package.json");
    await showNpm(page);
    await page.locator('.npm-package[data-package="package.json"] ~ .npm-script[data-script="build"]').first().click();
    await page.waitForTimeout(300);
  }),
  "webforge-npm-run": on("react", async (page) => {
    await ready(page, "React");
    await showNpm(page);
    await page.locator('.npm-script[data-script="dev"]').first().dblclick();
    await page.waitForSelector('[data-tool-window-panel="run"]');
    await page.waitForTimeout(1200);
  }),
  "webforge-npm-gutter-menu": on("react", async (page) => {
    await ready(page, "React");
    await openFile(page, "package.json");
    await page.waitForSelector(".cm-line-marker.marker-npm-run");
    await page.locator(".cm-line-marker.marker-npm-run").nth(1).click();
    await page.waitForSelector(".db-menu");
    await page.waitForTimeout(200);
  }),
  "webforge-react-goto": on("react", async (page) => {
    await ready(page, "React");
    await caretAt(page, "<CartDrawer open", 3);
    await page.keyboard.press("Control+KeyB");
    await page.waitForSelector('.editor-tab.active[title="src/components/CartDrawer.tsx"]');
    await page.waitForTimeout(500);
  }),
  "webforge-react-usages": on("react", async (page) => {
    await ready(page, "React");
    await openFile(page, "src/components/ui/Button.tsx");
    await caretAt(page, "export const Button", 14);
    await page.keyboard.press("Control+KeyB");
    await page.waitForSelector(".wf-chooser-row");
    await page.waitForTimeout(200);
  }),
  "webforge-vue-goto": on("vue", async (page) => {
    await ready(page, "Vue");
    await caretAt(page, ':trend="12"', 2);
    await page.keyboard.press("Control+KeyB");
    await page.waitForSelector('.editor-tab.active[title="src/components/StatCard.vue"]');
    await page.waitForTimeout(500);
  }),
  "webforge-angular-gutter": on("angular", async (page) => {
    await ready(page, "Angular");
    await page.waitForSelector(".cm-line-marker.marker-ng-styles");
    await page.locator(".cm-line-marker.marker-ng-styles").first().click();
    await page.waitForSelector(".wf-chooser-row");
    await page.waitForTimeout(200);
  }),
  "webforge-angular-template": on("angular", async (page) => {
    await ready(page, "Angular");
    await openFile(page, "src/app/users/user-list.component.ts");
    await caretAt(page, "(selected)", 3);
    await page.keyboard.press("Control+KeyB");
    await page.waitForSelector('.editor-tab.active[title="src/app/users/user-card.component.ts"]');
    await page.waitForTimeout(500);
  }),
  "webforge-settings-typescript": on("vue", async (page) => {
    await ready(page, "Vue");
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.typescript"]');
    await page.waitForSelector('[data-server="vue-language-server"]');
    await page.waitForTimeout(300);
  }),
  "webforge-settings-node": on("react", async (page) => {
    await ready(page, "React");
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.node"]');
    await page.waitForSelector('[data-setting="web.packageManager"]');
    await page.waitForTimeout(200);
  }),
  "webforge-npm-light": on("vue", async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await ready(page, "Vue");
    await openFile(page, "package.json");
    await showNpm(page);
    await page.locator('.npm-script[data-script="test:unit"]').first().click();
    await page.waitForTimeout(300);
  }),
};
