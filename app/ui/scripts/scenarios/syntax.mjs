/** Syntax highlighting of the non-PHP file types in the demo project. */

/** @param {import("playwright-core").Page} page @param {string} path */
async function openPath(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    // Expand only collapsed directories (their child rows are not rendered yet).
    if ((await page.locator(`.tree-row[data-path^="${dir}/"]`).count()) === 0) await row.dblclick();
  }
  await page.dblclick(`.tree-row[data-path="${path}"]`);
  await page.waitForTimeout(400);
}

const scenario = (path) => async (page) => openPath(page, path);

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "syntax-env": scenario(".env"),
  "syntax-yaml": scenario("docker-compose.yml"),
  "syntax-sql": scenario("database/schema.sql"),
  "syntax-json": scenario("composer.json"),
  "syntax-dockerfile": scenario("Dockerfile"),
  "syntax-typescript": scenario("resources/js/app.ts"),
  "syntax-xml": scenario("phpunit.xml"),
  "syntax-markdown": scenario("README.md"),
  "syntax-twig": scenario("templates/order/show.html.twig"),
  "syntax-smarty": scenario("legacy/templates/cart.tpl"),
  "syntax-html": scenario("public/index.html"),
  "syntax-css": scenario("resources/css/app.css"),
  "syntax-twig-latex": scenario("templates/invoice/invoice.tex.twig"),
  "syntax-twig-sql": scenario("templates/reports/top_customers.sql.twig"),
};
