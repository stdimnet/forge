/** Syntax highlighting of the non-PHP file types in the demo project. */

/** @param {import("playwright-core").Page} page @param {string} path */
async function openPath(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    // Expand only collapsed directories (their child rows are not rendered yet).
    if ((await page.locator(`.tree-row[data-path^="${dir}/"]`).count()) === 0) await row.click();
  }
  await page.click(`.tree-row[data-path="${path}"]`);
  await page.waitForTimeout(400);
}

const scenario = (path) => async (page) => openPath(page, path);

/** Put the caret at the end of the first editor line containing `text`. */
async function caretAtLineEnd(page, text) {
  await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first().click();
  await page.keyboard.press("End");
}

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
  "syntax-go-template": scenario("templates/mail/welcome.html.tmpl"),
  "syntax-compose-merge-tags": async (page) => {
    await openPath(page, "docker-compose.override.yml");
    await page.click('[data-tool-window="problems"]');
    await page.waitForTimeout(600);
  },
  "compose-tag-hover": async (page) => {
    await openPath(page, "docker-compose.override.yml");
    const point = await page.evaluate(() => {
      const lines = document.querySelectorAll(".editor-slot:not(.hidden) .cm-line");
      const line = [...lines].find((l) => l.textContent.includes("build: !reset"));
      const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        const i = n.textContent.indexOf("!reset");
        if (i < 0) continue;
        const r = document.createRange();
        r.setStart(n, i + 2);
        r.setEnd(n, i + 3);
        const b = r.getBoundingClientRect();
        return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
      }
      return null;
    });
    await page.mouse.move(point.x, point.y);
    await page.waitForSelector(".cm-tooltip-hover .lsp-hover");
  },
  "compose-tag-completion": async (page) => {
    await openPath(page, "docker-compose.override.yml");
    await caretAtLineEnd(page, "image: ghcr.io/acme/shop:latest");
    await page.keyboard.press("Enter");
    await page.keyboard.type("volumes: !", { delay: 30 });
    await page.waitForSelector(".cm-tooltip-autocomplete");
    await page.waitForTimeout(500);
  },
  "syntax-smarty-latex": scenario("legacy/templates/invoice.tex.tpl"),
  "syntax-twig-latex-letter": scenario("legacy/templates/letter.tex.twig"),
  "syntax-twig-latex-letter-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPath(page, "legacy/templates/letter.tex.twig");
  },
  "syntax-smarty-latex-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openPath(page, "legacy/templates/invoice.tex.tpl");
  },
};
