/** PyForge screenshots, on the FastAPI demo service (`?project=python`). */

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
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page */
async function showTool(page, id) {
  if (!(await page.locator(`[data-tool-window-panel="${id}"]`).isVisible().catch(() => false))) {
    await page.click(`[data-tool-window="${id}"]`);
  }
  await page.waitForSelector(`[data-tool-window-panel="${id}"]`);
  await page.waitForTimeout(300);
}

const scenario = (fn) => Object.assign(fn, { query: "?project=python" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // basedpyright's errors next to PyForge's "not listed in the requirements" inspection.
  "python-editor": scenario(async (page) => {
    await page.waitForSelector(".cm-lintRange, .cm-diagnostic-weak, .cm-lintRange-warning", { timeout: 8000 }).catch(() => {});
    await page.waitForTimeout(1200);
    await showTool(page, "problems");
    await page.waitForTimeout(600);
  }),
  "python-quick-fix": scenario(async (page) => {
    await page.waitForTimeout(1200);
    const line = page.locator(".cm-line", { hasText: "import yaml" }).first();
    await line.click();
    await page.keyboard.press("End");
    await page.keyboard.press("Home");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Alt+Enter");
    await page.waitForTimeout(800);
  }),
  "python-requirements": scenario(async (page) => {
    await openFile(page, "requirements-dev.txt");
    await page.waitForTimeout(1500);
    const line = page.locator(".cm-line", { hasText: "mypy" }).first();
    await line.click();
    await page.keyboard.press("Home");
    await page.keyboard.press("Alt+Enter");
    await page.waitForTimeout(800);
  }),
  "python-interpreter-popup": scenario(async (page) => {
    await page.waitForSelector('[data-status="python-interpreter"]');
    await page.waitForTimeout(600);
    await page.click('[data-status="python-interpreter"]');
    await page.waitForSelector(".context-menu");
    await page.waitForTimeout(400);
  }),
  "python-interpreter-settings": scenario(async (page) => {
    await page.waitForTimeout(800);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="project.python-interpreter"]');
    await page.waitForSelector('[data-testid="python-interpreter-page"]');
    await page.waitForTimeout(800);
  }),
  "python-add-interpreter": scenario(async (page) => {
    await page.waitForTimeout(600);
    await page.evaluate(() => window.__forgeRunCommand("python.addInterpreter"));
    await page.waitForSelector('[data-testid="python-add-interpreter"]');
    await page.selectOption("#py-new-kind", "uv");
    await page.waitForTimeout(400);
  }),
  "python-packages": scenario(async (page) => {
    await page.waitForTimeout(600);
    await page.evaluate(() => window.__forgeRunCommand("ActivatePythonPackagesToolWindow"));
    await page.waitForSelector('[data-testid="python-packages"] .py-package-row');
    await page.locator('.py-package-row[data-package="fastapi"]').click();
    await page.waitForSelector(".py-detail-summary");
    await page.waitForTimeout(500);
  }),
  "python-packages-search": scenario(async (page) => {
    await page.waitForTimeout(600);
    await page.evaluate(() => window.__forgeRunCommand("ActivatePythonPackagesToolWindow"));
    await page.waitForSelector('[data-testid="python-packages"] .py-package-row');
    await page.fill('[data-testid="python-packages-search"]', "ri");
    await page.waitForTimeout(700);
    await page.locator('.py-package-row[data-package="rich"]').click();
    await page.waitForSelector(".py-detail-summary");
    await page.waitForTimeout(400);
  }),
  "python-packages-light": scenario(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.waitForTimeout(600);
    await page.evaluate(() => window.__forgeRunCommand("ActivatePythonPackagesToolWindow"));
    await page.waitForSelector('[data-testid="python-packages"] .py-package-row');
    await page.locator('.py-package-row[data-package="httpx"]').click();
    await page.waitForSelector(".py-detail-summary");
    await page.waitForTimeout(400);
  }),
};
