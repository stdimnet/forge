/**
 * RustForge screenshots, on the `shop` demo workspace (`?project=rust`): the Rust editor with
 * test and doctest gutter icons, the Cargo tool window with features, a cargo test tree with
 * a failure and doctests, Clippy problems with a quick fix, an LLDB session with rustc's
 * pretty printers (Vec, String, Option, HashMap) and Settings | Rust.
 */

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path, line = 1) {
  await page.evaluate(
    async ([p, l]) => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt(p, l, 1, { flash: false });
    },
    [path, line],
  );
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
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

/** Click the gutter ▶ icon on the line containing `text` and pick a menu item. */
async function gutterRun(page, text, item) {
  await page.waitForSelector(".run-gutter-icon:visible");
  const line = page.locator(".cm-line:visible", { hasText: text }).first();
  const box = await line.boundingBox();
  const icons = page.locator(".cm-run-gutter:visible .cm-gutterElement");
  const count = await icons.count();
  for (let i = 0; i < count; i++) {
    const b = await icons.nth(i).boundingBox();
    if (b && box && Math.abs(b.y - box.y) < 4 && (await icons.nth(i).locator(".run-gutter-icon").count())) {
      await icons.nth(i).locator(".run-gutter-icon").click();
      break;
    }
  }
  if (item) await page.click(`.run-popup-menu .run-menu-item:has-text("${item}")`);
}

const rust = (fn) => Object.assign(fn, { query: "?project=rust" });

/** @param {import("playwright-core").Page} page */
async function runConfiguration(page, name, debug = false) {
  await page.evaluate(
    async ([n, d]) => {
      const { debugConfiguration, launch } = await import("/src/features/run/runStore.ts");
      const { findConfiguration } = await import("/src/features/run/configStore.ts");
      if (d) await debugConfiguration(findConfiguration(n));
      else await launch(findConfiguration(n));
    },
    [name, debug],
  );
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // cart.rs: ▶ on the doctest of Cart::total, the test module and its tests.
  "rust-editor": rust(async (page) => {
    await closeBalloons(page);
    await openFile(page, "crates/shop-core/src/cart.rs", 30);
    await page.waitForSelector(".run-gutter-icon:visible");
    await gutterRun(page, "/// ```");
    await page.waitForSelector(".run-popup-menu");
    await page.waitForTimeout(300);
  }),
  // The Cargo tool window: members, targets, features with checkboxes, dependencies.
  "rust-cargo": rust(async (page) => {
    await closeBalloons(page);
    await openFile(page, "crates/shop-core/Cargo.toml");
    await page.click('[data-tool-window="cargo"]');
    await page.waitForSelector('.cargo-feature[data-feature="discounts"]');
    await page.click('.cargo-feature[data-feature="discounts"]');
    await page.dblclick('[data-cargo-row="pkg:shop-cli/deps"]');
    await page.waitForTimeout(600);
  }),
  // All tests: plain libtest output of a stable toolchain read into the tree, doctests included.
  "rust-test-tree": rust(async (page) => {
    await closeBalloons(page);
    await openFile(page, "crates/shop-core/src/cart.rs", 66);
    await runConfiguration(page, "All tests");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => /failed/.test(document.querySelector(".run-tests-status")?.textContent ?? ""), null, { timeout: 20000 });
    await page.waitForTimeout(800);
    await bottomHeight(page, 360);
    await page.click('.test-row[data-test="cart::tests::rounds_to_cents"]').catch(() => {});
    await page.waitForTimeout(500);
  }),
  // Clippy: problems in the editor and the Problems tool window, Alt+Enter offers its fix.
  "rust-clippy": rust(async (page) => {
    await closeBalloons(page);
    await openFile(page, "crates/shop-core/src/money.rs", 14);
    await page.evaluate(() => window.__forgeRunCommand("rust.clippy"));
    await page.waitForSelector('[data-tool-window-panel="problems"]');
    await page.waitForTimeout(1500);
    await closeBalloons(page);
    const line = page.locator(".cm-line", { hasText: "(cents == 0) == true" }).first();
    await line.click({ position: { x: 90, y: 5 } });
    await page.keyboard.press("Alt+Enter");
    await page.waitForTimeout(800);
  }),
  // LLDB: Debug "shop" stopped in main; Vec, String, Option and HashMap shown as values.
  "rust-debug": rust(async (page) => {
    await closeBalloons(page);
    await openFile(page, "crates/shop-cli/src/main.rs", 12);
    const cell = page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: /^12$/ }).first();
    await cell.click();
    await page.waitForSelector(".cm-bp-dot");
    await runConfiguration(page, "shop", true);
    await page.waitForSelector(".cm-exec-line", { timeout: 15000 });
    await page.waitForSelector('.dbg-row[data-var="cart"]', { timeout: 5000 }).catch(() => {});
    await bottomHeight(page, 360);
    await page.click('.dbg-row[data-var="cart"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(400);
    await page.click('.dbg-row[data-var="cart.items"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(400);
    await page.click('.dbg-row[data-var="quantities"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(600);
  }),
  // Settings | Languages & Frameworks | Rust.
  "rust-settings": rust(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.rust"]');
    await page.waitForSelector('[data-testid="rust-toolchain"]');
    await page.waitForTimeout(800);
  }),
};
