/**
 * Navigate menu: Last/Next Edit Location, Next/Previous Method, Next/Previous Change (Git
 * change popup), Select In… (Alt+F1) and the Project / Structure views' autoscroll options.
 */

const run = (page, id) => page.evaluate((id) => window.__forgeRunCommand(id), id);

/** Put the caret at the end of 1-based `line` of the active editor (a visible line). @param {import("playwright-core").Page} page */
async function caretOnLine(page, line) {
  const box = await page.evaluate((line) => {
    for (const el of document.querySelectorAll(".editor-slot:not(.hidden) .cm-gutterElement")) {
      if (el.textContent?.trim() === String(line) && el.closest(".cm-lineNumbers")) {
        const r = el.getBoundingClientRect();
        return { y: r.top + r.height / 2 };
      }
    }
    throw new Error(`line ${line} is not visible`);
  }, line);
  const content = await page.locator(".editor-slot:not(.hidden) .cm-content").boundingBox();
  await page.mouse.click(content.x + content.width - 20, box.y);
  await page.keyboard.press("End");
  await page.waitForTimeout(100);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "select-in": async (page) => {
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Alt+F1");
    await page.waitForSelector(".select-in-popup .select-in-row");
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(150);
  },

  "select-in-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Alt+F1");
    await page.waitForSelector(".select-in-popup .select-in-row");
    await page.waitForTimeout(150);
  },

  "next-change": async (page) => {
    await page.waitForSelector(".editor-slot:not(.hidden) .cm-git-change", { timeout: 10000 });
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Control+Home");
    await page.keyboard.press("Control+Alt+Shift+ArrowDown");
    await page.waitForSelector(".git-change-popup");
    await page.waitForTimeout(200);
  },

  "next-method": async (page) => {
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Control+Home");
    await page.keyboard.press("Alt+ArrowDown");
    await page.keyboard.press("Alt+ArrowDown");
    await page.keyboard.press("Alt+ArrowDown");
    await page.waitForTimeout(200);
    const caret = await page.evaluate(() => document.querySelector(".status-bar")?.textContent ?? "");
    if (!/\d+:\d+/.test(caret)) throw new Error("no caret position in the status bar");
  },

  "last-edit-location": async (page) => {
    await page.click(".editor-slot:not(.hidden) .cm-content");
    for (const [line, text] of [[3, " // edit 1"], [24, " // edit 2"], [13, " (edit 3)"]]) {
      await caretOnLine(page, line);
      await page.keyboard.type(text);
    }
    await caretOnLine(page, 30);
    // Back to the newest edit, then the one before it.
    await page.keyboard.press("Control+Shift+Backspace");
    await page.waitForTimeout(200);
    await page.keyboard.press("Control+Shift+Backspace");
    await page.waitForTimeout(300);
    await page.keyboard.press("Control+Shift+E");
    await page.waitForSelector(".locations-popup");
    await page.keyboard.press("Control+Shift+E");
    await page.waitForFunction(() => document.querySelector(".locations-popup .recent-title")?.textContent === "Recently Changed Locations");
    await page.waitForTimeout(300);
  },

  "structure-autoscroll": async (page) => {
    await run(page, "ActivateStructureToolWindow");
    await page.waitForSelector(".php-structure .tree-row", { timeout: 10000 });
    await caretOnLine(page, 30);
    await page.waitForSelector(".php-structure .tree-row.selected", { timeout: 5000 });
    await page.click('[data-tool-window-panel="structure"] [aria-label="Options"]');
    await page.waitForTimeout(200);
  },
};
