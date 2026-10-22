/** Vim emulation (Tools | Vim) in User.php: modes, search highlights, the command line and
 * Settings | Editor | Vim. */

/** @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** Turn Vim on and put the caret on a line of User.php. @param {import("playwright-core").Page} page */
async function vimOn(page, lineText) {
  await page.evaluate(() => window.__forgeRunCommand("VimPluginToggle"));
  await page.waitForSelector(".vim-status");
  await page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: lineText }).first().click();
  await quiet(page);
}

/** Press Vim keys: plain characters, `<Escape>`, `<Control+v>`. @param {import("playwright-core").Page} page */
async function keys(page, text) {
  for (const m of text.matchAll(/<([^>]+)>|./gs)) {
    await page.keyboard.press(m[1] ?? (m[0] === " " ? "Space" : m[0]));
  }
  await page.waitForTimeout(100);
}

/** @param {import("playwright-core").Page} page */
async function openVimSettings(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.waitForSelector(".set-row, .plugin-item");
  await page.evaluate(() => {
    const input = document.querySelector(".set-dialog input");
    if (input) input.blur();
  });
  await page.click('.set-tree-row[data-node="editor.vim"]');
  await page.waitForSelector(".vim-handlers-table tr[data-shortcut]");
  await page.waitForTimeout(200);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "vim-emulation": async (page) => {
    await vimOn(page, "protected $fillable");
    // Search (hlsearch from the demo ~/.ideavimrc), then select three lines.
    await keys(page, "gg/protected<Enter>V2j");
  },
  "vim-command-line": async (page) => {
    await vimOn(page, "protected $fillable");
    await keys(page, ":%s/protected/private/g");
  },
  "vim-visual-block": async (page) => {
    await vimOn(page, "use Illuminate\\Database\\Eloquent\\Factories");
    await keys(page, "0w<Control+v>3j$");
  },
  "vim-settings": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("VimPluginToggle"));
    await page.waitForSelector(".vim-status");
    await quiet(page);
    await openVimSettings(page);
  },
};
