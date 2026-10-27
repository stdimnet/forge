/**
 * Inspections: Settings | Editor | Inspections, the Inspect Code scope dialog, Inspection
 * Results (problems and duplicates), Run Inspection by Name, Code Cleanup's preview and
 * Alt+Enter's Suppress actions on a data-flow problem.
 */

/** @param {import("playwright-core").Page} page @param {string} path */
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
  await page.waitForTimeout(1500);
}

/** @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(300);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** Click into the first occurrence of `text` in the visible editor. */
async function clickText(page, text, offset = 1, nth = 0) {
  const { x, y } = await page.evaluate(
    ([text, offset, nth]) => {
      const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
      if (!root) throw new Error("no editor");
      for (const line of root.querySelectorAll(".cm-line")) {
        const content = line.textContent ?? "";
        let at = content.indexOf(text);
        while (at >= 0) {
          if (nth-- === 0) {
            const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
            let rest = at + offset;
            let node = walker.nextNode();
            while (node && rest > (node.textContent ?? "").length) {
              rest -= (node.textContent ?? "").length;
              node = walker.nextNode();
            }
            const range = document.createRange();
            range.setStart(node, rest);
            range.setEnd(node, Math.min(rest + 1, (node.textContent ?? "").length));
            const r = range.getBoundingClientRect();
            return { x: r.left + 1, y: r.top + r.height / 2 };
          }
          at = content.indexOf(text, at + 1);
        }
      }
      throw new Error(`text not found: ${text}`);
    },
    [text, offset, nth],
  );
  await page.mouse.click(x, y);
  return { x, y };
}

/** @param {import("playwright-core").Page} page */
async function openInspectionsSettings(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click('.set-tree-row[data-node="editor.inspections"]');
  await page.waitForSelector(".insp-tree .insp-row");
  await page.waitForTimeout(400);
}

/** Inspect Code over the whole project. */
async function inspectProject(page) {
  await quiet(page);
  await page.evaluate(() => void window.__forgeRunCommand("InspectCode"));
  await page.waitForSelector(".insp-scope-dialog");
  await page.click('.insp-scope-dialog button.btn-primary');
  await page.waitForSelector(".ir-row");
  await page.waitForTimeout(600);
  await quiet(page);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "inspections-settings": async (page) => {
    await openInspectionsSettings(page);
    await page.fill(".insp-search-input", "php");
    await page.waitForTimeout(300);
    await page.click('.insp-row[data-inspection="PhpUnusedAssignment"]');
    await page.selectOption('.insp-details select[aria-label="Severity"]', "WARNING");
    await page.waitForTimeout(400);
  },
  "inspections-settings-scopes": async (page) => {
    await openInspectionsSettings(page);
    await page.fill(".insp-search-input", "duplicated");
    await page.waitForTimeout(300);
    await page.click('.insp-row[data-inspection="DuplicatedCode"]');
    await page.click('.insp-details button[title="Add Scope"]');
    await page.waitForSelector(".context-menu");
    await page.click('.context-menu >> text="Project Test Files"');
    await page.waitForTimeout(400);
  },
  "inspections-profile-menu": async (page) => {
    await openInspectionsSettings(page);
    await page.click(".insp-gear");
    await page.waitForSelector(".context-menu");
    await page.waitForTimeout(200);
  },
  "inspections-inspect-code-dialog": async (page) => {
    await openFile(page, "app/Services/LegacyPricing.php");
    await quiet(page);
    await page.evaluate(() => void window.__forgeRunCommand("InspectCode"));
    await page.waitForSelector(".insp-scope-dialog");
    await page.waitForTimeout(300);
  },
  "inspections-results": async (page) => {
    await inspectProject(page);
    // The data-flow problems of LegacyPricing.php.
    await page.locator('.ir-row[data-node="problem"]', { hasText: "always null" }).first().click();
    await page.waitForTimeout(400);
  },
  "inspections-results-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await inspectProject(page);
    await page.locator('.ir-row[data-node="problem"]', { hasText: "never used" }).first().click();
    await page.waitForTimeout(400);
  },
  "inspections-duplicates": async (page) => {
    await inspectProject(page);
    await page.click('.ir-mode >> text="Duplicates"');
    await page.waitForSelector(".ir-dup");
    await page.waitForTimeout(300);
  },
  "inspections-run-by-name": async (page) => {
    await quiet(page);
    await page.keyboard.press("Control+Alt+Shift+KeyI");
    await page.waitForSelector(".insp-by-name");
    await page.keyboard.type("unused");
    await page.waitForTimeout(300);
  },
  "inspections-code-cleanup": async (page) => {
    await quiet(page);
    await page.evaluate(() => void window.__forgeRunCommand("CodeCleanup"));
    await page.waitForSelector(".insp-scope-dialog");
    await page.click('.insp-scope-dialog button.btn-primary');
    await page.waitForSelector(".insp-cleanup");
    await page.click('.insp-cleanup-file >> text="LegacyPricing.php"');
    await page.waitForTimeout(800);
  },
  "inspections-suppress": async (page) => {
    await openFile(page, "app/Services/LegacyPricing.php");
    await quiet(page);
    // Line 50 is below the fold.
    await page.evaluate(() => {
      const scroller = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
      if (scroller) scroller.scrollTop = 620;
    });
    await page.waitForTimeout(500);
    await clickText(page, "$customer->", 3);
    await page.mouse.move(1380, 760);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup, [class*=intention]", { timeout: 4000 }).catch(() => undefined);
    await page.waitForTimeout(300);
    // The inspection's options and Suppress actions in the sub-menu.
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(400);
  },
};
