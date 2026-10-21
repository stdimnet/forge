/** Markdown preview (Mermaid, tables, images), the image editor, SVG preview and Edit as Table. */

/** Open a project file by double-clicking through the Project view. */
async function openPath(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    if ((await row.getAttribute("data-expanded")) === null) await row.locator(".tree-chevron").click();
    await page.waitForSelector(`.tree-row[data-path="${parts.slice(0, i + 1).join("/")}"]`);
  }
  await page.locator(`.tree-row[data-path="${path}"]`).dblclick();
  await page.waitForSelector(`.editor-tab.active[data-tab-id="${path}"]`);
}

/** Collapse `app` so the Project view shows the root files. */
async function collapseApp(page) {
  const app = page.locator('.tree-row[data-path="app"]');
  if ((await app.getAttribute("data-expanded")) !== null) await app.locator(".tree-chevron").click();
}

export default {
  "markdown-preview": async (page) => {
    await collapseApp(page);
    await openPath(page, "README.md");
    await page.waitForSelector(".mdp-table");
    await page.waitForSelector(".mdp-img");
    await page.waitForSelector(".mdp-mermaid img", { timeout: 20000 });
    await page.waitForTimeout(300);
  },
  "markdown-preview-scrolled": async (page) => {
    await collapseApp(page);
    await openPath(page, "README.md");
    await page.waitForSelector(".mdp-mermaid img", { timeout: 20000 });
    // Scroll the editor: the preview follows to the same block.
    await page.evaluate(() => {
      const scroller = document.querySelector(".twp-editor .cm-scroller");
      if (scroller) scroller.scrollTop = 420;
    });
    await page.waitForTimeout(400);
  },
  "markdown-preview-only-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await collapseApp(page);
    await openPath(page, "README.md");
    await page.click('.twp-btn[data-layout="preview"]');
    await page.waitForSelector(".mdp-mermaid img", { timeout: 20000 });
    await page.waitForTimeout(300);
  },
  "image-editor": async (page) => {
    await collapseApp(page);
    await openPath(page, "public/images/logo.png");
    await page.waitForSelector(".img-image");
    await page.click(".img-viewport");
    await page.keyboard.press("Control+Equal");
    await page.keyboard.press("Control+Equal");
    await page.waitForTimeout(200);
  },
  "svg-preview": async (page) => {
    await collapseApp(page);
    await openPath(page, "public/images/hero.svg");
    await page.waitForSelector(".twp-preview .img-image");
    await page.waitForTimeout(300);
  },
  "csv-table": async (page) => {
    await collapseApp(page);
    await openPath(page, "storage/app/orders.csv");
    await page.click('.csv-tab:has-text("Table")');
    await page.waitForSelector(".csv-table .dg-row");
    await page.click('.dg-th:has-text("total")');
    await page.waitForTimeout(300);
  },
  "markdown-settings": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-page, .settings-dialog");
    await page.locator('text="Markdown"').first().click();
    await page.waitForTimeout(300);
  },
};
