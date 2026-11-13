/**
 * RubyForge screenshots, on the `pantry` demo gem (`?project=ruby`): a spec file with RSpec
 * gutter icons, the Rake tool window, the RSpec test tree with a failure read from RSpec's
 * JSON report, Minitest results, RuboCop offenses with Alt+Enter autocorrect, an rdbg
 * session stopped at a breakpoint, an ERB view, External Libraries with the bundle's gems
 * and Settings | Ruby.
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
  await page.waitForTimeout(500);
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

/** Click the gutter ▶ icon on the line containing `text`. */
async function gutterRun(page, text) {
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
}

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

const ruby = (fn) => Object.assign(fn, { query: "?project=ruby" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // inventory_spec.rb: ▶ on the example group, nested groups and examples.
  "ruby-editor": ruby(async (page) => {
    await closeBalloons(page);
    await openFile(page, "spec/pantry/inventory_spec.rb", 11);
    await page.waitForSelector(".run-gutter-icon:visible");
    await gutterRun(page, "describe \"#total_cents\"");
    await page.waitForSelector(".run-popup-menu");
    await page.waitForTimeout(300);
  }),
  // The Rake tool window: tasks by namespace with arguments and descriptions.
  "ruby-rake": ruby(async (page) => {
    await closeBalloons(page);
    await openFile(page, "Rakefile", 18);
    await page.evaluate(() => window.__forgeRunCommand("ActivateRakeToolWindow"));
    await page.waitForSelector('[data-rake-row="task:db:seed"]');
    await page.click('[data-rake-row="task:shopping_list"]');
    await page.waitForTimeout(500);
  }),
  // All specs: RSpec's JSON report read into the tree, the failed example selected.
  "ruby-rspec": ruby(async (page) => {
    await closeBalloons(page);
    await openFile(page, "spec/pantry/inventory_spec.rb", 23);
    await runConfiguration(page, "All specs");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => /failed/.test(document.querySelector(".run-tests-status")?.textContent ?? ""), null, { timeout: 20000 });
    await page.waitForTimeout(800);
    await bottomHeight(page, 360);
    await page.click('.test-row[data-test="adds up what is missing"]').catch(() => {});
    await page.waitForTimeout(500);
  }),
  // All tests: Minitest's verbose output read into the tree.
  "ruby-minitest": ruby(async (page) => {
    await closeBalloons(page);
    await openFile(page, "test/pantry/item_test.rb", 19);
    await runConfiguration(page, "All tests");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => /failed/.test(document.querySelector(".run-tests-status")?.textContent ?? ""), null, { timeout: 20000 });
    await page.waitForTimeout(800);
    await bottomHeight(page, 320);
    await page.click('.test-row[data-test="test_price"]').catch(() => {});
    await page.waitForTimeout(500);
  }),
  // RuboCop: offenses in the editor and the Problems tool window, Alt+Enter autocorrect.
  "ruby-rubocop": ruby(async (page) => {
    await closeBalloons(page);
    await openFile(page, "lib/pantry/inventory.rb", 27);
    await page.evaluate(() => window.__forgeRunCommand("ruby.rubocop"));
    await page.waitForSelector('[data-tool-window-panel="problems"]');
    await page.waitForTimeout(1500);
    await closeBalloons(page);
    const line = page.locator(".cm-line", { hasText: "'Pantry with '" }).first();
    await line.click({ position: { x: 60, y: 5 } });
    await page.keyboard.press("Alt+Enter");
    await page.waitForTimeout(800);
  }),
  // rdbg: Debug "restock" stopped in Inventory#total_cents with its locals.
  "ruby-debug": ruby(async (page) => {
    await closeBalloons(page);
    await openFile(page, "lib/pantry/inventory.rb", 23);
    const cell = page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: /^23$/ }).first();
    await cell.click();
    await page.waitForSelector(".cm-bp-dot");
    await runConfiguration(page, "restock", true);
    await page.waitForSelector(".cm-exec-line", { timeout: 15000 });
    await bottomHeight(page, 340);
    await page.waitForSelector('.dbg-row[data-var="item"]', { timeout: 5000 }).catch(() => {});
    await page.click('.dbg-row[data-var="item"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(600);
  }),
  // An ERB view: HTML with Ruby tags; External Libraries lists the Ruby SDK and gems.
  "ruby-erb": ruby(async (page) => {
    await closeBalloons(page);
    await openFile(page, "app/views/inventory/index.html.erb", 1);
    await page.click('.tree-row[data-path="ruby-libraries://"] .tree-chevron');
    await page.waitForSelector('.tree-row[data-path="ruby-libraries://gems"]');
    await page.click('.tree-row[data-path="ruby-libraries://gems"] .tree-chevron');
    await page.waitForTimeout(600);
  }),
  // Settings | Languages & Frameworks | Ruby.
  "ruby-settings": ruby(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.ruby"]');
    await page.waitForSelector('[data-testid="ruby-interpreter"]');
    await page.waitForTimeout(800);
  }),
};
