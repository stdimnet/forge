/** Laravel plugin screenshots: route() completion, Routes window, Blade, artisan, gutter. */

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
  await page.waitForTimeout(250);
}

/** @param {import("playwright-core").Page} page */
async function caretAtLineEnd(page, text) {
  await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first().click();
  await page.keyboard.press("End");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "laravel-route-completion": async (page) => {
    await openFile(page, "app/Http/Controllers/CheckoutController.php");
    await caretAtLineEnd(page, "'total' => $request->input('total'),");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Enter");
    await page.keyboard.type("$next = route('", { delay: 30 });
    await page.waitForSelector(".cm-tooltip-autocomplete li");
  },
  "laravel-column-completion": async (page) => {
    await openFile(page, "app/Http/Controllers/ProductController.php");
    await caretAtLineEnd(page, "$products = Product::available()");
    await page.keyboard.press("Enter");
    await page.keyboard.type("$cheap = Product::query()->where('", { delay: 20 });
    await page.waitForSelector(".cm-tooltip-autocomplete li");
  },
  "laravel-goto-chooser": async (page) => {
    await openFile(page, "app/Http/Controllers/CheckoutController.php");
    await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "redirect()->route('orders.show'" }).first().click();
    await page.keyboard.press("End");
    for (let i = 0; i < 12; i++) await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Control+KeyB");
    await page.waitForSelector(".decl-chooser-row");
  },
  "laravel-goto-view": async (page) => {
    await openFile(page, "app/Http/Controllers/ProductController.php");
    await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: "return view('products.index'" }).first().click();
    await page.keyboard.press("End");
    for (let i = 0; i < 30; i++) await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Control+KeyB");
    await page.waitForSelector('.editor-tab.active[title="resources/views/products/index.blade.php"]');
  },
  "laravel-routes": async (page) => {
    await page.evaluate(async () => {
      // Dev server only: give the bottom panel room for the whole table.
      const layout = await import("/src/state/layout.ts");
      layout.resizePanel("bottom", 430);
      await window.__forgeRunCommand("Laravel.ShowRoutes");
    });
    await page.waitForSelector(".routes-table tbody tr");
    await page.locator('.routes-table tr[data-route="checkout.store"]').click();
  },
  "laravel-blade": async (page) => {
    await openFile(page, "resources/views/orders/show.blade.php");
  },
  "laravel-blade-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openFile(page, "resources/views/orders/show.blade.php");
  },
  "laravel-blade-directives": async (page) => {
    await openFile(page, "resources/views/products/show.blade.php");
    await caretAtLineEnd(page, "<p>@money($product->price)</p>");
    await page.keyboard.press("Enter");
    await page.keyboard.type("@for", { delay: 40 });
    await page.waitForSelector(".cm-tooltip-autocomplete li");
  },
  "laravel-gutter": async (page) => {
    await openFile(page, "app/Http/Controllers/ProductController.php");
    await page.waitForSelector(".laravel-gutter-icon");
    await page.locator('.laravel-gutter-icon[data-laravel-gutter="route"]').first().hover();
    await page.waitForTimeout(900);
  },
  "laravel-artisan": async (page) => {
    await page.keyboard.press("Control+Shift+KeyX");
    await page.waitForSelector(".artisan-row");
    await page.keyboard.type("make", { delay: 30 });
  },
  "laravel-artisan-output": async (page) => {
    await page.keyboard.press("Control+Shift+KeyX");
    await page.waitForSelector(".artisan-row");
    await page.keyboard.type("route:list", { delay: 20 });
    await page.keyboard.press("Enter");
    await page.waitForSelector(".artisan-status.ok");
  },
  "laravel-settings": async (page) => {
    // What clicking the "Laravel 12" status item does.
    await page.click('[data-status="laravel"]');
    await page.waitForSelector(".set-dialog .laravel-settings");
    await page.waitForTimeout(300);
  },
};
