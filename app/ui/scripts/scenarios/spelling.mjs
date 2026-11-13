/**
 * Spelling and grammar: typos (green squiggles) and grammar problems in comments, strings
 * and declared names, the Alt+Enter fixes, the Problems tool window, the commit message and
 * the Natural Languages settings pages.
 */

/**
 * Put the caret into the `nth` occurrence of `text` in the visible editor, `offset`
 * characters in.
 * @param {import("playwright-core").Page} page @param {string} text
 */
async function caretAt(page, text, nth = 0, offset = 1) {
  await page.evaluate(
    async ([text, nth, offset]) => {
      const { visibleView } = await import("/src/editor/views.ts");
      const { activeTab } = await import("/src/state/editor.ts");
      const view = await visibleView(activeTab().path);
      const doc = view.state.doc.toString();
      let at = -1;
      for (let i = 0; i <= nth; i++) at = doc.indexOf(text, at + 1);
      if (at < 0) throw new Error(`text not found: ${text}`);
      view.dispatch({ selection: { anchor: at + offset }, scrollIntoView: true });
      view.focus();
    },
    [text, nth, offset],
  );
}

/** User.php with a few mistakes typed in. @param {import("playwright-core").Page} page */
async function withMistakes(page) {
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
  await page.evaluate(async () => {
    const { visibleView } = await import("/src/editor/views.ts");
    const { activeTab } = await import("/src/state/editor.ts");
    const view = await visibleView(activeTab().path);
    const doc = view.state.doc.toString();
    const insert = (after, text) => {
      const at = view.state.doc.toString().indexOf(after) + after.length;
      view.dispatch({ changes: { from: at, insert: text } });
    };
    void doc;
    insert(" * An authenticated customer of the shop.", "\n * Recieve the the orders of a user and an website adress.");
    insert("protected $hidden = ['password', 'remember_token'];", "\n\n    protected $ordrTotl = 0;\n\n    protected $greeting = 'Thank you for your pruchase!';");
  });
  await page.mouse.move(1380, 760);
  await page.waitForTimeout(1500);
}

/** @param {import("playwright-core").Page} page */
async function intentions(page) {
  await page.keyboard.press("Alt+Enter");
  await page.waitForSelector(".intention-popup", { timeout: 5000 });
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page @param {string} id */
async function openSettingsPage(page, id) {
  await page.evaluate(async (id) => {
    const { openSettings } = await import("/src/features/settings/dialogState.ts");
    openSettings(id);
  }, id);
  await page.waitForSelector(".set-page", { timeout: 5000 });
  await page.waitForTimeout(800);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "spelling-editor": async (page) => {
    await withMistakes(page);
    await caretAt(page, "Recieve", 0, 3);
    await page.mouse.move(1380, 760);
    await page.waitForTimeout(400);
  },
  "spelling-intentions": async (page) => {
    await withMistakes(page);
    await caretAt(page, "Recieve", 0, 3);
    await intentions(page);
  },
  "spelling-change-to": async (page) => {
    await withMistakes(page);
    await caretAt(page, "Recieve", 0, 3);
    await intentions(page);
    await page.keyboard.press("Enter");
    await page.waitForTimeout(500);
  },
  "spelling-rename-to": async (page) => {
    await withMistakes(page);
    await caretAt(page, "ordrTotl", 0, 2);
    await intentions(page);
    await page.keyboard.press("Enter");
    await page.waitForTimeout(500);
  },
  "spelling-grammar": async (page) => {
    await withMistakes(page);
    await caretAt(page, "an website", 0, 1);
    await intentions(page);
  },
  "spelling-problems": async (page) => {
    await withMistakes(page);
    await page.keyboard.press("Alt+6");
    await page.waitForSelector(".problems-view .problem-row", { timeout: 5000 });
    await page.waitForTimeout(300);
  },
  "spelling-commit-message": async (page) => {
    await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
    await page.keyboard.press("Alt+0");
    await page.waitForSelector(".commit-message textarea", { timeout: 5000 });
    await page.click(".commit-message textarea");
    await page.keyboard.type("Fix the the totl of an order\n\nRecalculate totals when an discount changes.");
    await page.waitForSelector("[data-spell-problem]", { timeout: 5000 });
    await page.waitForTimeout(300);
  },
  "spelling-settings": async (page) => {
    await openSettingsPage(page, "editor.natural-languages.spelling");
  },
  "spelling-natural-languages": async (page) => {
    await openSettingsPage(page, "editor.natural-languages");
  },
};
