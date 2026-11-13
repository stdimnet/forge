/** Structural Search and Replace, Analyze Stack Trace. */

/** @param {import("playwright-core").Page} page @param {string} id */
async function run(page, id) {
  await page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);
}

const TRACE = `Illuminate\\Database\\Eloquent\\ModelNotFoundException: No query results for model [App\\Models\\Order] 42 in /var/www/html/vendor/laravel/framework/src/Illuminate/Database/Eloquent/Builder.php:621
Stack trace:
#0 /var/www/html/app/Http/Controllers/OrderController.php(28): Illuminate\\Database\\Eloquent\\Builder->findOrFail(42)
#1 /var/www/html/app/Services/Checkout.php(19): App\\Http\\Controllers\\OrderController->show(42)
#2 /var/www/html/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): App\\Services\\Checkout->run()
#3 /var/www/html/public/index.php(52): Illuminate\\Foundation\\Http\\Kernel->handle(Object(Illuminate\\Http\\Request))
#4 {main}`;

/** Close the demo's start-up balloons so they do not cover the results. @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const b of await page.locator(".balloon-close").all()) await b.click().catch(() => undefined);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "structural-search": async (page) => {
    await run(page, "StructuralSearchPlugin.StructuralSearchAction");
    await page.waitForSelector(".ssr-dialog");
    await page.click('.ssr-library-item[data-template="new expressions"]');
    await page.waitForSelector(".ssr-var-chip");
    await page.waitForTimeout(400);
  },
  "structural-search-results": async (page) => {
    await run(page, "StructuralSearchPlugin.StructuralSearchAction");
    await page.waitForSelector(".ssr-dialog");
    await page.click('.ssr-library-item[data-template="method calls"]');
    await page.waitForTimeout(400);
    await page.getByRole("button", { name: "Find", exact: true }).click();
    await page.waitForSelector(".find-usage");
    await closeBalloons(page);
  },
  "structural-replace": async (page) => {
    await run(page, "StructuralSearchPlugin.StructuralReplaceAction");
    await page.waitForSelector(".ssr-dialog");
    await page.evaluate(async () => {
      const { setSsrForm } = await import("/src/features/ssr/state.ts");
      setSsrForm({
        template: "$this->assertSame($Expected$, $Actual$)",
        replacement: "self::assertSame($Expected$, $Actual$)",
        constraints: {},
      });
    });
    await page.waitForTimeout(400);
  },
  "structural-replace-preview": async (page) => {
    await run(page, "StructuralSearchPlugin.StructuralReplaceAction");
    await page.waitForSelector(".ssr-dialog");
    await page.evaluate(async () => {
      const { setSsrForm } = await import("/src/features/ssr/state.ts");
      setSsrForm({ template: "$table->string($Column$)", replacement: "$table->string($Column$)->nullable()", constraints: {} });
    });
    await page.waitForTimeout(400);
    await page.getByRole("button", { name: "Replace", exact: true }).click();
    await page.waitForSelector(".find-actions");
    await closeBalloons(page);
  },
  "analyze-stack-trace": async (page) => {
    await run(page, "AnalyzeStacktraceOnError");
    await page.waitForSelector(".stacktrace-dialog");
    await page.fill(".stacktrace-text", TRACE);
  },
  "stack-trace-tab": async (page) => {
    await run(page, "AnalyzeStacktraceOnError");
    await page.waitForSelector(".stacktrace-dialog");
    await page.fill(".stacktrace-text", TRACE);
    await page.getByRole("button", { name: "Analyze" }).click();
    await page.waitForSelector(".run-console .run-link");
    await closeBalloons(page);
  },
};
