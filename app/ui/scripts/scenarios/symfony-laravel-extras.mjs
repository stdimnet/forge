/**
 * Symfony translations, forms and events (`?project=symfony`) and the Laravel tool window
 * (overview, logs) on the Laravel demo shop.
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
  await page.locator(`.tree-row[data-path="${path}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function caretAtLineEnd(page, text) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first();
  await line.scrollIntoViewIfNeeded();
  await line.click();
  await page.keyboard.press("End");
}

/** @param {import("playwright-core").Page} page */
async function ready(page) {
  await page.waitForSelector(".status-item", { hasText: "Symfony" }).catch(() => {});
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function showTool(page, id, tab, height = 380) {
  await page.evaluate(async ([tw, h]) => {
    const layout = await import("/src/state/layout.ts");
    layout.resizePanel("bottom", h);
    layout.showToolWindow(tw);
  }, [id, height]);
  await page.waitForSelector(`[data-tool-window-panel="${id}"]`);
  if (tab) await page.locator(".sf-tab", { hasText: tab }).first().click();
  await page.waitForTimeout(400);
}

const symfony = (fn) => Object.assign(fn, { query: "?project=symfony" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "symfony-translation-folding": symfony(async (page) => {
    await ready(page);
    await openFile(page, "templates/order/show.html.twig");
    await page.waitForSelector(".editor-slot:not(.hidden) .cm-sf-translation-fold");
    // The caret on the missing key shows it with its warning; Alt+Enter offers the fix.
    await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "shipping_note" }).first().click();
    await page.keyboard.press("End");
    for (let i = 0; i < 20; i++) await page.keyboard.press("ArrowLeft");
    await page.waitForTimeout(300);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup");
    await page.waitForTimeout(300);
  }),
  "symfony-translation-completion": symfony(async (page) => {
    await ready(page);
    await openFile(page, "templates/order/show.html.twig");
    // Replace `shipping_note` in `'order.shipping_note'|trans` by typing `st`.
    await caretAtLineEnd(page, "shipping_note");
    for (let i = 0; i < 14; i++) await page.keyboard.press("ArrowLeft");
    for (let i = 0; i < 13; i++) await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.type("st", { delay: 40 });
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(300);
  }),
  "symfony-translations-tab": symfony(async (page) => {
    await ready(page);
    await showTool(page, "symfony", "Translations");
    await page.waitForSelector(".sf-translations tbody tr");
  }),
  "symfony-events": symfony(async (page) => {
    await ready(page);
    await openFile(page, "src/EventSubscriber/LocaleSubscriber.php");
    await showTool(page, "symfony", "Events", 300);
    await page.waitForSelector(".sf-events tbody tr");
    await page.waitForSelector(".marker-sf-listener");
  }),
  "symfony-form-options": symfony(async (page) => {
    await ready(page);
    await openFile(page, "src/Form/ProductType.php");
    await caretAtLineEnd(page, "['currency' => 'EUR', 'divisor' => 100]");
    for (let i = 0; i < 2; i++) await page.keyboard.press("ArrowLeft");
    await page.keyboard.type(", 'gr", { delay: 30 });
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(300);
  }),
  "laravel-tool-window-overview": async (page) => {
    await page.evaluate(async () => {
      const layout = await import("/src/state/layout.ts");
      layout.resizePanel("bottom", 470);
      await window.__forgeRunCommand("Laravel.ShowDashboard");
    });
    await page.waitForSelector(".lv-overview .lv-card");
    await page.waitForTimeout(400);
  },
  "laravel-logs": async (page) => {
    await page.evaluate(async () => {
      const layout = await import("/src/state/layout.ts");
      layout.resizePanel("bottom", 470);
      await window.__forgeRunCommand("Laravel.ShowLogs");
    });
    await page.waitForSelector(".lv-entry");
    await page.locator(".lv-entry[data-level=error] .lv-entry-head").click();
    await page.waitForSelector(".lv-frame");
    await page.waitForTimeout(300);
  },
  "laravel-logs-light": async (page) => {
    await page.evaluate(async () => {
      document.documentElement.dataset.theme = "light";
      const layout = await import("/src/state/layout.ts");
      layout.resizePanel("bottom", 470);
      await window.__forgeRunCommand("Laravel.ShowLogs");
    });
    await page.waitForSelector(".lv-entry");
    await page.locator(".lv-entry[data-level=error] .lv-entry-head").click();
    await page.waitForSelector(".lv-frame");
    await page.waitForTimeout(300);
  },
};
