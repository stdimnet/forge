/** Terminal, Run/Debug configurations and Run tool window screenshots. */

/** @param {import("playwright-core").Page} page */
async function type(page, text) {
  await page.keyboard.type(text, { delay: 8 });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(120);
}

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  const y = box.y + box.height / 2;
  const target = 900 - 26 - height;
  await page.mouse.move(box.x + 200, y);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, target, { steps: 4 });
  await page.mouse.up();
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await page.locator(`.tree-row[data-path="${dir}"]`).click();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page */
async function terminalTabs(page) {
  await page.keyboard.press("Alt+F12");
  await page.waitForSelector(".term-host .xterm-screen");
  await bottomHeight(page, 340);
  await page.waitForTimeout(250);
  await type(page, "php -v");
  await type(page, "git status");
  await page.click(".term-tabs .term-new");
  await page.waitForTimeout(250);
  await type(page, "ls");
  await type(page, "grep -rn orders app");
  await page.click('.term-tab[data-terminal-tab="Local"]');
  await page.waitForTimeout(200);
}

/** @param {import("playwright-core").Page} page */
async function runConfigurationsDialog(page) {
  await page.click(".run-config");
  await page.click(".run-dropdown .run-dd-item:has-text('Edit Configurations')");
  await page.waitForSelector(".rc-dialog");
  await page.click('.rc-tree-item[data-config="UserTest"]');
  await page.waitForTimeout(150);
}

/** Run UserTest from its gutter icon and select the failed test. */
async function runPhpunit(page) {
  await openFile(page, "tests/Unit/UserTest.php");
  await page.waitForSelector(".run-gutter-icon");
  await page.locator(".run-gutter-icon").first().click();
  await page.click(".run-popup-menu .run-menu-item:has-text(\"Run 'UserTest'\")");
  await page.waitForSelector(".run-tests-status");
  await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("Tests failed"), null, {
    timeout: 10000,
  });
  await bottomHeight(page, 330);
  await page.click('.test-row[data-test="test_display_name_prefers_nickname"]');
  await page.waitForTimeout(200);
}

/** @param {import("playwright-core").Page} page */
async function toolbarDropdown(page) {
  await page.click(".run-config");
  await page.waitForSelector(".run-dropdown");
  await page.hover('.run-dropdown .run-dd-item[data-config="artisan serve"]');
  await page.click('.run-dropdown .run-dd-item[data-config="artisan serve"] .run-btn');
  await page.waitForSelector(".run-console .run-line");
  await page.waitForTimeout(2600);
  await page.click(".run-config");
  await page.waitForSelector(".run-dropdown");
  await page.hover('.run-dropdown .run-dd-item[data-config="UserTest"]');
}

export default {
  "terminal-tabs": terminalTabs,
  "run-configurations-dialog": runConfigurationsDialog,
  "run-phpunit-tree": runPhpunit,
  "run-toolbar-dropdown": toolbarDropdown,
};
