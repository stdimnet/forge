/**
 * Mouse back/forward buttons (PHPStorm's Button4/Button5 Click) run Navigate | Back and
 * Forward (#222). The clicks are real browser input through the DevTools protocol, so they
 * pass through Chromium's own handling of the side buttons (history navigation) as in WebView2.
 * The scenarios throw when the editor does not navigate.
 */

/** @param {import("playwright-core").Page} page @param {string} path */
async function openFile(page, path) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1, 1, { flash: false });
  }, path);
  await page.waitForFunction((p) => document.querySelector(".editor-tab.active, .editor-tab[aria-selected='true']")?.textContent?.includes(p.split("/").pop()), path);
}

/** @param {import("playwright-core").Page} page */
const activePath = (page) =>
  page.evaluate(async () => {
    const { editor } = await import("/src/state/editor.ts");
    return editor.active;
  });

/**
 * Press and release a side button over the editor, as the OS reports it to Chromium.
 * @param {import("playwright-core").Page} page @param {"back" | "forward"} button
 */
async function sideClick(page, button) {
  const box = await page.locator(".cm-content:visible").first().boundingBox();
  const at = { x: Math.round(box.x + 80), y: Math.round(box.y + 40) };
  const cdp = await page.context().newCDPSession(page);
  for (const type of ["mousePressed", "mouseReleased"]) {
    await cdp.send("Input.dispatchMouseEvent", { type, ...at, button, clickCount: 1 });
  }
  await cdp.detach();
  await page.waitForTimeout(200);
}

/** @param {import("playwright-core").Page} page @param {string} want @param {string} step */
async function expectActive(page, want, step) {
  const got = await activePath(page);
  if (got !== want) throw new Error(`${step}: expected ${want} to be active, got ${got}`);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "navigate-mouse-back-forward": async (page) => {
    const url = page.url();
    await openFile(page, "routes/web.php");
    await openFile(page, "app/Models/User.php");
    await sideClick(page, "back");
    await expectActive(page, "routes/web.php", "mouse back");
    if (page.url() !== url) throw new Error(`mouse back navigated the page to ${page.url()}`);
    await sideClick(page, "forward");
    await expectActive(page, "app/Models/User.php", "mouse forward");
    await sideClick(page, "back");
    await expectActive(page, "routes/web.php", "mouse back again");
  },
  "keymap-mouse-back-forward": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.click(".set-tree-row:has-text('Keymap')");
    await page.waitForSelector(".keymap-page");
    await page.fill(".km-search input", "Back");
    await page.waitForTimeout(300);
    await page.click(".km-action[data-action='Back']");
    const row = await page.locator(".km-action[data-action='Back']").textContent();
    if (!row?.includes("Button4")) throw new Error(`Back shows no mouse shortcut: ${row}`);
  },
};
