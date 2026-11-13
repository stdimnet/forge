/**
 * Code | Folding | Collapse (Ctrl+Minus) folds the innermost region around the caret from
 * anywhere inside it, Expand (Ctrl+=) unfolds it (#218). Real key presses through the keymap;
 * the scenario throws when the editor does not fold.
 */

/** @param {import("playwright-core").Page} page @param {string} path @param {number} line @param {number} col */
async function openAt(page, path, line, col) {
  await page.evaluate(
    async ([p, l, c]) => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt(p, l, c, { flash: false });
    },
    [path, line, col],
  );
  await page.waitForTimeout(400);
}

/** The first line of every folded range in the active editor. @param {import("playwright-core").Page} page */
const folded = (page) =>
  page.evaluate(async () => {
    const { activeView } = await import("/src/editor/views.ts");
    const { foldedRanges } = await import("/node_modules/.vite/deps/@codemirror_language.js").catch(() => import("@codemirror/language"));
    const view = activeView();
    const out = [];
    foldedRanges(view.state).between(0, view.state.doc.length, (from, to) => {
      out.push(view.state.sliceDoc(from, to).trim().split("\n")[0]);
    });
    return out;
  });

/** @param {import("playwright-core").Page} page @param {string} line */
async function lineOfText(page, line) {
  return page.evaluate(async (text) => {
    const { activeView } = await import("/src/editor/views.ts");
    const doc = activeView().state.doc;
    for (let n = 1; n <= doc.lines; n++) if (doc.line(n).text.includes(text)) return n;
    return -1;
  }, line);
}

/** @param {unknown} got @param {unknown} want @param {string} step */
function expectEqual(got, want, step) {
  if (JSON.stringify(got) !== JSON.stringify(want)) throw new Error(`${step}: expected ${JSON.stringify(want)}, got ${JSON.stringify(got)}`);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "folding-collapse-from-inside": async (page) => {
    await openAt(page, "app/Models/User.php", 1, 1);
    // Caret inside the method body: Ctrl+Minus folds the method.
    await openAt(page, "app/Models/User.php", await lineOfText(page, "return $this->hasMany"), 12);
    await page.keyboard.press("Control+Minus");
    await page.waitForTimeout(200);
    expectEqual(await folded(page), ["return $this->hasMany(Order::class);"], "collapse from the body");
    await page.keyboard.press("Control+Equal");
    await page.waitForTimeout(200);
    expectEqual(await folded(page), [], "expand");
    // Caret on the signature line (brace on the next line): Ctrl+Minus folds the method.
    await openAt(page, "app/Models/User.php", await lineOfText(page, "public function totalSpent"), 10);
    await page.keyboard.press("Control+Minus");
    await page.waitForTimeout(300);
    expectEqual(await folded(page), ["return $this->orders"], "collapse from the signature");
  },
};
