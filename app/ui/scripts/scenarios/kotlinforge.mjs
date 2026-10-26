/**
 * KotlinForge screenshots on the Gradle demo service (`?project=kotlin`): the Kotlin editor
 * with kotlin-lsp completion, the Run gutter on `main`, a Kotest tree from Gradle's reports,
 * a Kotlin debug session with the Coroutines tab, ktlint and detekt in the Problems view,
 * Settings | Kotlin and the kotlin-lsp download offer.
 */

const kotlin = (fn) => Object.assign(fn, { query: "?project=kotlin" });
const PKG = "src/main/kotlin/com/acme/orders";
const TEST_PKG = "src/test/kotlin/com/acme/orders";

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1, 1);
  }, path);
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** Expand the project tree down to `path`'s folder (a click toggles a folder). */
async function reveal(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await page.locator(`.tree-row[data-path="${dir}"]`).first().click();
    await page.waitForTimeout(120);
  }
}

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + 200, y);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, 900 - 26 - height, { steps: 4 });
  await page.mouse.up();
}

async function caretAfter(page, text) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first();
  await line.click();
  await page.keyboard.press("End");
}

/** Click the test gutter icon on the line containing `text` and pick `item`. */
async function testGutter(page, text, item) {
  const line = page.locator(".cm-line:visible", { hasText: text }).first();
  const box = await line.boundingBox();
  const icons = page.locator(".cm-run-gutter:visible .cm-gutterElement");
  for (let i = 0; i < (await icons.count()); i++) {
    const b = await icons.nth(i).boundingBox();
    if (b && box && Math.abs(b.y - box.y) < 4 && (await icons.nth(i).locator(".run-gutter-icon").count())) {
      await icons.nth(i).locator(".run-gutter-icon").click();
      break;
    }
  }
  await page.waitForSelector(".run-popup-menu");
  if (item) await page.click(`.run-popup-menu .run-menu-item:has-text("${item}")`);
}

async function breakpoint(page, line) {
  await page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: new RegExp(`^${line}$`) }).first().click();
  await page.waitForSelector(".cm-bp-dot");
}

const MAIN_MARKER = `.cm-line-marker[title="Run 'MainKt'"]`;

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "kotlin-editor-completion": kotlin(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${PKG}/Main.kt`);
    await openFile(page, `${PKG}/Main.kt`);
    await page.waitForSelector(MAIN_MARKER, { timeout: 10000 });
    await caretAfter(page, "val total = service.checkout(order)");
    await page.keyboard.press("Enter");
    await page.keyboard.type("order.");
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(500);
  }),
  "kotlin-run-gutter": kotlin(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${PKG}/Main.kt`);
    await openFile(page, `${PKG}/Main.kt`);
    const marker = page.locator(MAIN_MARKER).first();
    await marker.waitFor({ timeout: 10000 });
    await marker.click();
    await page.waitForSelector(".run-popup-menu");
    await page.waitForTimeout(300);
  }),
  "kotlin-kotest-tree": kotlin(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${TEST_PKG}/CartSpec.kt`);
    await openFile(page, `${TEST_PKG}/CartSpec.kt`);
    await page.waitForSelector(".run-gutter-icon:visible", { timeout: 10000 });
    await testGutter(page, "class CartSpec", "Run 'CartSpec'");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("failed"), null, { timeout: 20000 });
    await bottomHeight(page, 330);
    await page.locator(".test-row", { hasText: "applies the discount" }).first().click().catch(() => {});
    await page.waitForTimeout(500);
  }),
  "kotlin-debug-coroutines": kotlin(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${PKG}/Main.kt`);
    await openFile(page, `${PKG}/Main.kt`);
    await breakpoint(page, 20);
    const marker = page.locator(MAIN_MARKER).first();
    await marker.waitFor({ timeout: 10000 });
    await marker.click();
    await page.click(`.run-popup-menu .run-menu-item:has-text("Debug 'MainKt'")`);
    await page.waitForSelector(".cm-exec-line", { timeout: 30000 });
    await bottomHeight(page, 360);
    await page.click('.dbg-tab[data-tab="coroutines"]');
    await page.waitForSelector(".kt-coroutine-row", { timeout: 10000 });
    await page.waitForTimeout(600);
  }),
  "kotlin-ktlint-detekt-problems": kotlin(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${PKG}/OrderService.kt`);
    await openFile(page, `${PKG}/Main.kt`);
    await openFile(page, `${PKG}/OrderService.kt`);
    await page.evaluate(() => window.__forgeRunCommand("kotlin.inspectCode"));
    await page.waitForTimeout(1500);
    await page.click('[data-tool-window="problems"]');
    await page.click(".problems-tab:nth-child(1)").catch(() => {});
    await bottomHeight(page, 300);
    await page.waitForTimeout(3000);
    await closeBalloons(page);
    await page.waitForTimeout(400);
  }),
  "kotlin-settings": kotlin(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("lang.kotlin");
    });
    await page.waitForSelector("[data-kotlin-status]");
    await page.waitForTimeout(500);
  }),
  "kotlin-settings-light": kotlin(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("lang.kotlin");
    });
    await page.waitForSelector("[data-kotlin-status]");
    await page.waitForTimeout(500);
  }),
  "kotlin-lsp-install": Object.assign(
    async (page) => {
      await page.waitForSelector(".balloon", { timeout: 15000 });
      await page.waitForTimeout(400);
    },
    { query: "?project=kotlin&kotlin-lsp=missing" },
  ),
};
