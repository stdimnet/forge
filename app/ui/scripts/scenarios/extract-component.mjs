/**
 * JS/TS refactorings: Extract Component for Vue (vue-dashboard demo) and React
 * (storefront-ui demo) and Refactor This on a JSX selection.
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
  await page.waitForTimeout(400);
}

/** Put the caret `offset` characters after `needle` on the first line that contains it. */
async function caretAt(page, needle, offset = 0) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: needle }).first();
  await line.scrollIntoViewIfNeeded();
  await line.click();
  const text = await line.innerText();
  await page.keyboard.press("Home");
  // Home stops at the indentation; go to column 0 first.
  const indent = text.length - text.trimStart().length;
  const column = text.indexOf(needle) + offset;
  for (let i = 0; i < Math.abs(column - indent); i++) await page.keyboard.press(column > indent ? "ArrowRight" : "ArrowLeft");
}

/** @param {import("playwright-core").Page} page */
async function ready(page, label) {
  await page.waitForSelector(".status-item", { hasText: label }).catch(() => {});
  await page.waitForTimeout(400);
}

const on = (project, fn) => Object.assign(fn, { query: `?project=${project}` });

/** Select from the caret to the end of the line `lines` lines below. @param {import("playwright-core").Page} page */
async function selectLines(page, lines) {
  await page.keyboard.down("Shift");
  for (let i = 0; i < lines; i++) await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  await page.keyboard.up("Shift");
  await page.mouse.move(1380, 760);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "extract-component-vue-dialog": on("vue", async (page) => {
    await ready(page, "Vue");
    await caretAt(page, '<section class="stats">', 0);
    await selectLines(page, 4);
    await page.evaluate(() => window.__forgeRunCommand("WebForge.ExtractComponent"));
    await page.waitForSelector(".extract-component-dialog");
    await page.keyboard.press("Control+A");
    await page.keyboard.type("StatsRow");
    await page.waitForTimeout(300);
  }),
  "refactor-this-react": on("react", async (page) => {
    await ready(page, "React");
    await openFile(page, "src/components/CartDrawer.tsx");
    await caretAt(page, "<ul>", 0);
    await selectLines(page, 6);
    await page.keyboard.press("Control+Alt+Shift+KeyT");
    await page.waitForSelector(".refactor-this .intention-row");
    await page.waitForTimeout(300);
  }),
  "extract-component-react-result": on("react", async (page) => {
    await ready(page, "React");
    await openFile(page, "src/components/CartDrawer.tsx");
    await caretAt(page, "<ul>", 0);
    await selectLines(page, 6);
    await page.evaluate(() => window.__forgeRunCommand("WebForge.ExtractComponent"));
    await page.waitForSelector(".extract-component-dialog");
    await page.keyboard.press("Control+A");
    await page.keyboard.type("CartItems");
    await page.keyboard.press("Enter");
    await page.waitForSelector('.editor-tab.active[title="src/components/CartItems.tsx"]');
    await page.waitForTimeout(600);
  }),
};
