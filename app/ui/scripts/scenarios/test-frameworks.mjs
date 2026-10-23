/** PHP test frameworks: Pest, Behat, Codeception and PHPSpec runs, Test History, auto-test and settings. */

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

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1, 1);
  }, path);
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(250);
}

/** Click the gutter icon on the line containing `text` and pick a menu item. */
async function gutterRun(page, text, item) {
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
  await page.click(`.run-popup-menu .run-menu-item:has-text("${item}")`);
}

async function waitForTests(page, text = "Tests failed") {
  await page.waitForSelector(".run-tests-status");
  await page.waitForFunction((t) => document.querySelector(".run-tests-status")?.textContent?.includes(t), text, { timeout: 15000 });
}

/** Run the Pest file from a gutter icon; the icons show the results afterwards. */
async function pestRun(page) {
  await openFile(page, "tests/Feature/CartTest.php");
  await page.waitForSelector(".run-gutter-icon:visible");
  await gutterRun(page, "describe('totals'", "Run 'totals'");
  await waitForTests(page, "Tests");
  await page.waitForTimeout(1500);
  await bottomHeight(page, 330);
  await page.click('.test-row[data-test="`totals` → it rounds to cents"]');
  await page.waitForTimeout(300);
}

/** Behat: Gherkin highlighting, scenario gutter icons and a run with the JUnit report. */
async function behatRun(page) {
  await openFile(page, "features/checkout.feature");
  await page.waitForSelector(".run-gutter-icon:visible");
  await gutterRun(page, "Feature: Checkout", "Run 'Checkout'");
  await waitForTests(page);
  await bottomHeight(page, 300);
  await page.click('.test-row[data-test="Buying a single product"]');
  await page.waitForTimeout(300);
}

/** Ctrl+Click on a step opens its definition in the context class. */
async function behatStep(page) {
  await openFile(page, "features/checkout.feature");
  await page.evaluate(async () => {
    const { activeView } = await import("/src/features/lsp/navigation.ts");
    const { gotoDeclaration } = await import("/src/features/lsp/goto.ts");
    const a = activeView();
    const line = a.view.state.doc.line(12);
    await gotoDeclaration(a.path, a.view, line.from + 10);
  });
  await page.waitForSelector('.editor-tab.active[title="features/bootstrap/FeatureContext.php"]');
  await page.waitForTimeout(400);
}

/** Codeception and PHPSpec runs (JUnit reports) next to each other, auto-test on. */
async function codeceptionRun(page) {
  await openFile(page, "tests/Acceptance/LoginCest.php");
  await page.waitForSelector(".run-gutter-icon:visible");
  await gutterRun(page, "class LoginCest", "Run 'LoginCest'");
  await waitForTests(page);
  await bottomHeight(page, 300);
  await page.click(".run-auto-test");
  await page.click('.test-row[data-test="loginWithWrongPassword"]');
  await page.waitForTimeout(300);
}

async function phpspecRun(page) {
  await openFile(page, "spec/App/Services/CheckoutSpec.php");
  await page.waitForSelector(".run-gutter-icon:visible");
  await gutterRun(page, "class CheckoutSpec", "Run 'CheckoutSpec'");
  await waitForTests(page);
  await bottomHeight(page, 300);
  await page.click('.test-row[data-test="it rejects negative amounts"]');
  await page.waitForTimeout(300);
}

/** Two runs, then the Test History popup. */
async function testHistory(page) {
  await pestRun(page);
  await openFile(page, "spec/App/Services/CheckoutSpec.php");
  await page.waitForSelector(".run-gutter-icon:visible");
  await gutterRun(page, "class CheckoutSpec", "Run 'CheckoutSpec'");
  await waitForTests(page);
  await page.waitForTimeout(500);
  await page.click(".run-test-history");
  await page.waitForSelector(".run-popup-menu");
  await page.waitForTimeout(200);
}

/** Settings | PHP | Test Frameworks with the detected frameworks. */
async function settingsPage(page) {
  await page.evaluate(async () => {
    const { openSettings } = await import("/src/features/settings/dialogState.ts");
    openSettings("lang.php.test-frameworks");
  });
  await page.waitForSelector(".tf-item");
  await page.click('.tf-item[data-framework="pest"]');
  await page.waitForTimeout(250);
}

/** A Pest test saved from the gutter, in the Run/Debug Configurations dialog. */
async function pestConfiguration(page) {
  await openFile(page, "tests/Feature/CartTest.php");
  await page.waitForSelector(".run-gutter-icon:visible");
  await gutterRun(page, "it('rounds to cents'", "Run '");
  await waitForTests(page);
  await page.evaluate(async () => {
    const { saveTemporary, selectedConfiguration } = await import("/src/features/run/configStore.ts");
    const { openRunConfigurations } = await import("/src/features/run/dialogState.ts");
    const name = selectedConfiguration().name;
    await saveTemporary(name);
    openRunConfigurations(name);
  });
  await page.waitForSelector(".rc-dialog");
  await page.waitForTimeout(300);
}

/** The Behat run in the light theme. */
async function behatLight(page) {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await behatRun(page);
}

export default {
  "test-frameworks-pest": pestRun,
  "test-frameworks-behat": behatRun,
  "test-frameworks-behat-step": behatStep,
  "test-frameworks-light": behatLight,
  "test-frameworks-codeception": codeceptionRun,
  "test-frameworks-phpspec": phpspecRun,
  "test-frameworks-history": testHistory,
  "test-frameworks-settings": settingsPage,
  "test-frameworks-pest-configuration": pestConfiguration,
};
