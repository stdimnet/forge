/** LuaForge screenshots, on the LÖVE demo game (`?project=lua`). */

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
  await page.waitForTimeout(300);
}

/** Center of the `nth` occurrence of `text` in the active editor. */
async function textPoint(page, text, nth = 0, offset = 0.5) {
  return page.evaluate(
    ({ text, nth, offset }) => {
      const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
      let seen = 0;
      for (const line of root.querySelectorAll(".cm-line")) {
        const idx = line.textContent.indexOf(text);
        if (idx < 0) continue;
        if (seen++ < nth) continue;
        let remaining = idx + Math.floor(text.length * offset);
        const lw = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
        for (let n = lw.nextNode(); n; n = lw.nextNode()) {
          if (remaining <= n.length) {
            const r = document.createRange();
            r.setStart(n, Math.min(remaining, n.length - 1));
            r.setEnd(n, Math.min(remaining, n.length - 1) + 1);
            const b = r.getBoundingClientRect();
            return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
          }
          remaining -= n.length;
        }
      }
      return null;
    },
    { text, nth, offset },
  );
}

/** @param {import("playwright-core").Page} page */
async function showTool(page, id) {
  if (!(await page.locator(`[data-tool-window-panel="${id}"]`).isVisible().catch(() => false))) {
    await page.click(`[data-tool-window="${id}"]`);
  }
  await page.waitForSelector(`[data-tool-window-panel="${id}"]`);
  await page.waitForTimeout(300);
}

const scenario = (fn) => Object.assign(fn, { query: "?project=lua" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "lua-structure": scenario(async (page) => {
    await openFile(page, "src/player.lua");
    await page.keyboard.press("Alt+Digit7");
    await page.waitForSelector(".php-structure .tree-row");
    await page.locator(".php-structure .tree-row", { hasText: "move" }).first().click();
    await page.waitForTimeout(300);
  }),
  "lua-hover": scenario(async (page) => {
    await page.waitForTimeout(400);
    const p = await textPoint(page, "player:move", 0, 0.85);
    await page.mouse.move(p.x, p.y);
    await page.waitForSelector(".cm-tooltip-hover .lsp-hover");
    await page.waitForTimeout(300);
  }),
  "lua-hover-class": scenario(async (page) => {
    await openFile(page, "src/player.lua");
    const p = await textPoint(page, "local Player", 0, 0.8);
    await page.mouse.move(p.x, p.y);
    await page.waitForSelector(".cm-tooltip-hover .lsp-hover");
    await page.waitForTimeout(300);
  }),
  "lua-goto-require": scenario(async (page) => {
    await page.waitForTimeout(400);
    // Ctrl+B on `require("src.world")` opens the module…
    const p = await textPoint(page, "src.world");
    await page.mouse.click(p.x, p.y);
    await page.keyboard.press("Control+KeyB");
    await page.waitForSelector('.editor-tab.active[title="src/world.lua"]');
    await page.waitForTimeout(300);
  }),
  "lua-goto-method": scenario(async (page) => {
    await page.waitForTimeout(400);
    // …and Ctrl+Click on `world:isWalkable` jumps to the method through `---@type World`.
    const p = await textPoint(page, "isWalkable");
    await page.keyboard.down("Control");
    await page.mouse.move(p.x, p.y);
    await page.mouse.click(p.x, p.y);
    await page.keyboard.up("Control");
    await page.waitForSelector('.editor-tab.active[title="src/world.lua"]');
    await page.waitForTimeout(500);
  }),
  "lua-search-class": scenario(async (page) => {
    await page.waitForTimeout(400);
    await page.keyboard.press("Control+KeyN");
    await page.waitForSelector(".se-popup input, .search-everywhere input");
    await page.keyboard.type("pl");
    await page.waitForTimeout(600);
  }),
  "lua-run": scenario(async (page) => {
    await page.waitForTimeout(500);
    // The ▶ icon on the main chunk of the LÖVE game.
    await page.waitForSelector(".cm-line-marker.marker-run");
    await page.locator(".cm-line-marker.marker-run").first().dispatchEvent("mousedown");
    await page.waitForSelector(".context-menu");
    await page.locator(".context-menu", { hasText: "Run 'pixel-dungeon'" }).first().waitFor();
    await page.locator(".context-menu *", { hasText: /^Run 'pixel-dungeon'$/ }).last().click();
    await showTool(page, "run");
    // Give the Run tool window more room, as a user would by dragging the splitter.
    const splitter = page.locator(".splitter-h").first();
    const box = await splitter.boundingBox();
    if (box) {
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width / 2, box.y - 150, { steps: 5 });
      await page.mouse.up();
    }
    await page.waitForTimeout(2500);
  }),
  "lua-settings": scenario(async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.lua"]');
    await page.waitForTimeout(300);
  }),
};
