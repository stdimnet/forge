/**
 * Emmet and color previews: abbreviation preview and expansion in a Blade view, CSS
 * abbreviations, Surround with Emmet, the Settings | Editor | Emmet pages, gutter color
 * swatches and the color picker.
 */

/** @param {import("playwright-core").Page} page @param {string} path */
async function openFile(page, path) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1);
  }, path);
  await page.waitForTimeout(500);
}

/** @param {import("playwright-core").Page} page @param {string} key @param {unknown} value */
async function setting(page, key, value) {
  await page.evaluate(
    async ([k, v]) => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting(k, v);
    },
    [key, value],
  );
}

/** Caret at the end of the first line containing `text`. @param {import("playwright-core").Page} page */
async function endOfLine(page, text) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first();
  await line.click();
  await page.keyboard.press("End");
  await page.mouse.move(1380, 860);
}

/** @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** @param {import("playwright-core").Page} page */
async function openSettingsPage(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.waitForSelector(".set-row, .plugin-item");
  const row = page.locator(`.set-tree-row[data-node="${id}"]`);
  if (!(await row.count())) {
    const parent = id.split(".").slice(0, -1).join(".");
    await page.click(`.set-tree-row[data-node="${parent}"] .set-tree-chevron`);
  }
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForTimeout(200);
}

/** A Blade view with a new line ready for an abbreviation. @param {import("playwright-core").Page} page */
async function bladeLine(page) {
  await quiet(page);
  await openFile(page, "resources/views/welcome.blade.php");
  await endOfLine(page, "<h1>");
  await page.keyboard.press("Enter");
}

const on = (project, fn) => Object.assign(fn, { query: `?project=${project}` });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "emmet-preview": async (page) => {
    await setting(page, "editor.emmet.html.preview", true);
    await bladeLine(page);
    await page.keyboard.type("nav>ul.menu>li.menu-item*3>a{Item $}", { delay: 15 });
    await page.waitForSelector(".emmet-preview");
    await page.waitForTimeout(200);
  },
  "emmet-expand": async (page) => {
    await bladeLine(page);
    await page.keyboard.type("nav>ul.menu>li.menu-item*3>a{Item $}", { delay: 5 });
    await page.keyboard.press("Tab");
    await page.waitForTimeout(300);
  },
  "emmet-surround": async (page) => {
    await quiet(page);
    await openFile(page, "resources/views/welcome.blade.php");
    await endOfLine(page, "<h1>");
    await page.keyboard.press("Home");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Control+Alt+KeyJ");
    await page.waitForSelector('.lt-popup-item[data-item="emmet"]');
    await page.click('.lt-popup-item[data-item="emmet"]');
    await page.waitForSelector(".emmet-popup-input");
    await page.keyboard.type("header.hero>.container", { delay: 10 });
    await page.waitForSelector(".emmet-popup-preview");
    await page.waitForTimeout(200);
  },
  "emmet-css": on("react", async (page) => {
    await quiet(page);
    await openFile(page, "src/styles/app.css");
    await endOfLine(page, "padding: 12px;");
    for (const abbr of ["df", "jcsb", "m10-20", "bgc#f", "pos:a"]) {
      await page.keyboard.press("Enter");
      await page.keyboard.type(abbr, { delay: 10 });
      await page.keyboard.press("Tab");
      await page.keyboard.press("Escape");
      await page.keyboard.press("End");
    }
    await page.waitForTimeout(300);
  }),
  "emmet-settings": async (page) => {
    await openSettingsPage(page, "editor.emmet.html");
  },
  "color-preview": on("react", async (page) => {
    await quiet(page);
    await openFile(page, "src/styles/app.css");
    await page.waitForSelector(".color-swatch");
    await page.waitForTimeout(300);
  }),
  "color-picker": on("react", async (page) => {
    await setting(page, "editor.colorPicker.recentColors", ["#6d28d9", "#ec4899", "#0ea5e9", "#22c55e", "#f59e0b", "#ef4444"]);
    await quiet(page);
    await openFile(page, "src/styles/app.css");
    await page.waitForSelector(".color-swatch");
    await page.locator(".cm-line-marker.marker-color").nth(4).click();
    await page.waitForSelector(".color-picker");
    await page.waitForTimeout(200);
  }),
  "color-picker-light": on("react", async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await quiet(page);
    await setting(page, "editor.colorPreview.inline", true);
    await openFile(page, "src/styles/app.css");
    await page.waitForSelector(".color-swatch");
    await page.locator(".cm-line-marker.marker-color").first().click();
    await page.waitForSelector(".color-picker");
    // Drag in the saturation/brightness square: the color is written back as hex.
    const box = await page.locator(".cp-sv").boundingBox();
    await page.mouse.click(box.x + box.width * 0.8, box.y + box.height * 0.3);
    await page.waitForTimeout(300);
  }),
};
