/**
 * Live templates: expansion with Tab, Insert Live Template (Ctrl+J), Surround With
 * (Ctrl+Alt+T), Unwrap/Remove (Ctrl+Shift+Delete), Save as Live Template and the
 * Settings | Editor | Live Templates page.
 */

/**
 * Put the caret at the end of the line that contains `text` in the visible editor.
 * @param {import("playwright-core").Page} page @param {string} text
 */
async function endOfLine(page, text) {
  const { x, y } = await page.evaluate((text) => {
    const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
    if (!root) throw new Error("no editor");
    for (const line of root.querySelectorAll(".cm-line")) {
      if ((line.textContent ?? "").includes(text)) {
        const r = line.getBoundingClientRect();
        return { x: r.left + 4, y: r.top + r.height / 2 };
      }
    }
    throw new Error(`text not found: ${text}`);
  }, text);
  await page.mouse.click(x, y);
  await page.keyboard.press("End");
  await page.mouse.move(1380, 860);
  await page.waitForTimeout(100);
}

/** Close unrelated balloons and scroll `totalSpent()` into view. @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
  await page.evaluate(() => {
    const scroller = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
    if (scroller) scroller.scrollTop = 330;
  });
  await page.waitForTimeout(150);
}

/** A new line after the one containing `text`. @param {import("playwright-core").Page} page @param {string} text */
async function newLineAfter(page, text) {
  await endOfLine(page, text);
  await page.keyboard.press("Enter");
}

/** Select whole lines from the line containing `from` to the line containing `to`. */
async function selectLines(page, from, to) {
  await endOfLine(page, from);
  await page.keyboard.press("Home");
  const lines = await page.evaluate(
    ([from, to]) => {
      const rows = [...document.querySelectorAll(".editor-slot:not(.hidden) .cm-content .cm-line")].map((l) => l.textContent ?? "");
      return rows.findIndex((l) => l.includes(to)) - rows.findIndex((l) => l.includes(from));
    },
    [from, to],
  );
  for (let i = 0; i < lines; i++) await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Shift+End");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "live-template-expand": async (page) => {
    await quiet(page);
    await endOfLine(page, "public function totalSpent(): float");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("$orders = $this->orders()->get();", { delay: 5 });
    await page.keyboard.press("Enter");
    await page.keyboard.type("fore", { delay: 20 });
    await page.keyboard.press("Escape");
    await page.keyboard.press("Tab");
    await page.waitForTimeout(300);
  },
  "live-template-insert": async (page) => {
    await quiet(page);
    await newLineAfter(page, "protected $hidden");
    await page.keyboard.press("Enter");
    await page.keyboard.type("pu", { delay: 20 });
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+KeyJ");
    await page.waitForSelector(".lt-popup .lt-popup-item");
    await page.waitForTimeout(200);
  },
  "surround-with": async (page) => {
    await quiet(page);
    await selectLines(page, "return $this->orders", "->sum(fn");
    await page.keyboard.press("Control+Alt+KeyT");
    await page.waitForSelector(".lt-popup .lt-popup-item");
    await page.waitForTimeout(200);
  },
  "unwrap-remove": async (page) => {
    await quiet(page);
    await selectLines(page, "return $this->orders", "->sum(fn");
    await page.keyboard.press("Control+Alt+KeyT");
    await page.waitForSelector(".lt-popup .lt-popup-item");
    await page.keyboard.press("2");
    await page.waitForTimeout(200);
    await page.keyboard.type("$this->orders->isNotEmpty()", { delay: 5 });
    await endOfLine(page, "->where('status'");
    await page.keyboard.press("Control+Shift+Delete");
    await page.waitForSelector(".lt-popup .lt-popup-item");
    await page.waitForTimeout(200);
  },
  "save-as-live-template": async (page) => {
    await quiet(page);
    await selectLines(page, "return $this->orders", "->sum(fn");
    await page.evaluate(() => window.__forgeRunCommand("SaveAsTemplate"));
    await page.waitForSelector(".lt-editor #lt-abbr");
    await page.keyboard.type("spent", { delay: 10 });
    await page.fill("#lt-desc", "Sum of paid orders");
    await page.waitForTimeout(300);
  },
  "settings-live-templates": async (page) => {
    await quiet(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-tree-row");
    await page.click('.set-tree-row[data-node="editor.liveTemplates"]');
    await page.waitForSelector('.lt-group-row[data-group="PHP"]');
    await page.dblclick('.lt-group-row[data-group="PHP"]');
    await page.click('.lt-template-row[data-template="PHP/fore"]');
    await page.click("text=Edit Variables");
    await page.click(".lt-link");
    await page.evaluate(() => {
      const content = document.querySelector(".set-content");
      if (content) content.scrollTop = content.scrollHeight;
    });
    await page.waitForTimeout(300);
  },
  "unwrap-remove-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.waitForTimeout(400);
    await quiet(page);
    await page.waitForTimeout(300);
    await selectLines(page, "return $this->orders", "->sum(fn");
    await page.keyboard.press("Control+Alt+KeyT");
    await page.waitForSelector(".lt-popup .lt-popup-item");
    await page.keyboard.press("6");
    await page.waitForTimeout(200);
    await page.keyboard.type("$this->orders as $order", { delay: 5 });
    await endOfLine(page, "->where('status'");
    await page.keyboard.press("Control+Shift+Delete");
    await page.waitForSelector(".lt-popup .lt-popup-item");
    await page.waitForTimeout(200);
  },
};
