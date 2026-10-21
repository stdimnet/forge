/** Multiple carets, Column Selection Mode and Paste from History in User.php. */

/** @param {import("playwright-core").Page} page */
const line = (page, text) => page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first();

/** Put the caret `offset` characters after the indentation of a line. */
async function clickAt(page, text, offset) {
  await line(page, text).click();
  await page.keyboard.press("End");
  await page.keyboard.press("Home");
  for (let i = 0; i < offset; i++) await page.keyboard.press("ArrowRight");
}

/** Double Ctrl, keep it down, then Up/Down (Clone Caret Above / Below). */
async function cloneCaret(page, key, times) {
  await page.keyboard.down("Control");
  await page.keyboard.up("Control");
  await page.keyboard.down("Control");
  for (let i = 0; i < times; i++) await page.keyboard.press(key);
  await page.keyboard.up("Control");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "multiple-carets": async (page) => {
    // Add Selection for Next Occurrence (Alt+J) of `$this`, three times.
    await clickAt(page, "return $this->hasMany", 9);
    for (let i = 0; i < 3; i++) await page.keyboard.press("Alt+KeyJ");
    await page.waitForTimeout(200);
  },
  "clone-carets": async (page) => {
    // Clone the caret after `use ` down the imports and type a leading backslash.
    await clickAt(page, "Factories\\HasFactory;", 4);
    await cloneCaret(page, "ArrowDown", 3);
    await page.keyboard.type("\\", { delay: 30 });
    await page.waitForTimeout(200);
  },
  "column-selection-mode": async (page) => {
    await line(page, "@property int $id").click();
    await page.keyboard.press("Alt+Shift+Insert");
    await page.keyboard.press("Home");
    await page.keyboard.press("Home");
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
    for (let i = 0; i < 2; i++) await page.keyboard.press("Shift+ArrowDown");
    for (let i = 0; i < 10; i++) await page.keyboard.press("Shift+ArrowRight");
    await page.waitForTimeout(200);
  },
  "paste-from-history": async (page) => {
    // A few copies: a line, a selection with several carets and a statement.
    await line(page, "use HasFactory, Notifiable;").click();
    await page.keyboard.press("Control+KeyC");
    await line(page, "protected $hidden").click();
    await page.keyboard.press("Home");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Control+KeyC");
    await line(page, "public function orders()").click();
    await page.keyboard.press("Home");
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Control+KeyC");
    await clickAt(page, "protected $fillable", 2);
    await page.keyboard.press("Control+Alt+Shift+KeyJ");
    await page.keyboard.press("Control+KeyC");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+Shift+KeyV");
    await page.waitForSelector(".paste-history-row");
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(200);
  },
};

scenarios["paste-from-history-light"] = async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await scenarios["paste-from-history"](page);
};

export default scenarios;
