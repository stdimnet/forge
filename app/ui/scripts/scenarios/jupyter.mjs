/**
 * Jupyter notebooks on the FastAPI demo (`?project=python`): `notebooks/sales.ipynb` with
 * Markdown, code, a DataFrame table, a plot and an error; running cells on the simulated
 * kernel with the Variables view; View as DataFrame; the notebook diff against HEAD.
 */

/** @param {import("playwright-core").Page} page */
async function openNotebook(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
  await page.evaluate(async () => {
    const { openFile } = await import("/src/state/editor.ts");
    await openFile("notebooks/sales.ipynb");
  });
  await page.waitForSelector('[data-testid="notebook-cell"]');
  await page.waitForSelector('[data-testid="dataframe"] .dg');
  await page.waitForTimeout(500);
}

const python = (fn) => Object.assign(fn, { query: "?project=python" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // The notebook as saved: Markdown, the stdout of a cell, the DataFrame table.
  "jupyter-notebook": python(async (page) => {
    await openNotebook(page);
    await page.click('[data-cell-index="1"] .nb-cell-gutter');
  }),
  // Further down: the plot, a result and the KeyError traceback.
  "jupyter-outputs": python(async (page) => {
    await openNotebook(page);
    await page.click('[data-cell-index="4"] .nb-cell-gutter');
    await page.locator('[data-cell-index="4"]').scrollIntoViewIfNeeded();
    await page.evaluate(() => document.querySelector('[data-cell-index="3"]')?.scrollIntoView({ block: "start" }));
    await page.waitForTimeout(300);
  }),
  // Run All on the simulated kernel, with the Variables view.
  "jupyter-kernel": python(async (page) => {
    await openNotebook(page);
    await page.evaluate(() => window.__forgeRunCommand("Jupyter.ShowVariables"));
    await page.evaluate(() => window.__forgeRunCommand("Jupyter.RunAll"));
    await page.waitForFunction(() => document.querySelector(".nb-kernel")?.textContent?.includes("Idle") && !document.querySelector(".nb-cell.running"), null, { timeout: 15000 });
    await page.waitForSelector(".nb-var");
    await page.click('[data-cell-index="2"] .nb-cell-gutter');
    await page.waitForTimeout(400);
  }),
  // View as DataFrame from the Variables view: sorted by revenue.
  "jupyter-dataframe": python(async (page) => {
    await openNotebook(page);
    await page.evaluate(() => window.__forgeRunCommand("Jupyter.ShowVariables"));
    await page.evaluate(() => window.__forgeRunCommand("Jupyter.RunAll"));
    await page.waitForSelector(".nb-var-view", { timeout: 15000 });
    await page.click(".nb-var-view");
    await page.waitForSelector(".nb-frame-tab .dg");
    await page.locator(".nb-frame-tab .dg-th", { hasText: "revenue" }).first().click().catch(() => {});
    await page.waitForTimeout(400);
  }),
  // The diff against HEAD, cell by cell.
  "jupyter-diff": python(async (page) => {
    await openNotebook(page);
    await page.evaluate(async () => {
      const { showDiff } = await import("/src/features/git/actions.ts");
      await showDiff("notebooks/sales.ipynb");
    });
    await page.waitForSelector('[data-testid="notebook-diff-cell"]');
    await page.waitForTimeout(400);
  }),
};
