/**
 * PyForge web frameworks: the Django bookshop (`?project=django`) with its URLs in the
 * Endpoints tool window, template navigation and inspections, `{% url %}` completion, the
 * Django Structure tool window, Run manage.py Task… and a Celery worker run configuration,
 * plus the FastAPI service's endpoints (`?project=python`).
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
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page */
async function showTool(page, id) {
  if (!(await page.locator(`[data-tool-window-panel="${id}"]`).isVisible().catch(() => false))) {
    await page.click(`[data-tool-window="${id}"]`);
  }
  await page.waitForSelector(`[data-tool-window-panel="${id}"]`);
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function clickInLine(page, hasText, offsetText) {
  const line = page.locator(".cm-line", { hasText }).first();
  await line.scrollIntoViewIfNeeded();
  const box = await line.boundingBox();
  const full = (await line.textContent()) ?? "";
  const idx = full.indexOf(offsetText);
  // Monospace: columns × character width.
  const charWidth = await page.evaluate(() => {
    const el = document.querySelector(".cm-content");
    const span = document.createElement("span");
    span.textContent = "x".repeat(100);
    el?.appendChild(span);
    const w = span.getBoundingClientRect().width / 100;
    span.remove();
    return w;
  });
  await page.mouse.click(box.x + (idx + 3) * charWidth + 4, box.y + box.height / 2);
}

const django = (fn) => Object.assign(fn, { query: "?project=django", waitFor: ".cm-content" });
const fastapi = (fn) => Object.assign(fn, { query: "?project=python" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "django-endpoints": django(async (page) => {
    await showTool(page, "endpoints");
    await page.waitForSelector('[data-endpoint*="/books/"]');
    await page.waitForTimeout(400);
  }),
  // catalog/views.py: template and routed-view gutter icons, the unresolved template warning.
  "django-views-gutter": django(async (page) => {
    await page.waitForSelector(".marker-pyweb-template", { timeout: 10000 });
    await page.waitForTimeout(1200);
    await showTool(page, "problems");
    // Jump to the warning from the Problems tool window.
    await page.locator("text=author_detail.html' not found").first().dblclick();
    await page.waitForTimeout(800);
  }),
  // Ctrl+B on a template name in render() opens the template; its gutter shows the views.
  "django-template-navigation": django(async (page) => {
    await page.waitForSelector(".marker-pyweb-template", { timeout: 10000 });
    await clickInLine(page, '"catalog/book_detail.html"', "catalog/book_detail.html");
    await page.keyboard.press("Control+KeyB");
    await page.waitForSelector('.editor-tab.active[title="catalog/templates/catalog/book_detail.html"]', { timeout: 8000 });
    await page.waitForSelector(".marker-pyweb-view", { timeout: 8000 }).catch(() => {});
    await page.waitForTimeout(800);
  }),
  "django-url-completion": django(async (page) => {
    await openFile(page, "catalog/templates/catalog/book_detail.html");
    const line = page.locator(".cm-line", { hasText: "All books" }).first();
    await line.click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("  <a href=\"{% url 'catalog:b", { delay: 30 });
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 8000 });
    await page.waitForTimeout(500);
  }),
  "django-structure": django(async (page) => {
    await showTool(page, "djangoStructure");
    await page.waitForSelector('[data-django-node="app:catalog"]');
    await page.waitForTimeout(300);
  }),
  "django-manage-task": django(async (page) => {
    await page.waitForTimeout(800);
    await page.evaluate(() => window.__forgeRunCommand("Django.RunManageTask"));
    await page.waitForSelector(".pyweb-manage-item");
    await page.keyboard.type("m", { delay: 50 });
    await page.waitForTimeout(400);
  }),
  "celery-run-configuration": django(async (page) => {
    await page.waitForTimeout(600);
    await page.click(".run-config");
    await page.click(".run-dropdown .run-dd-item:has-text('Edit Configurations')");
    await page.waitForSelector(".rc-dialog");
    await page.click('.rc-tree-item[data-config="Celery worker"]');
    await page.waitForTimeout(400);
  }),
  "fastapi-endpoints": fastapi(async (page) => {
    await showTool(page, "endpoints");
    await page.waitForSelector('[data-endpoint*="/api/v1/customers"]');
    await page.waitForTimeout(400);
  }),
};
