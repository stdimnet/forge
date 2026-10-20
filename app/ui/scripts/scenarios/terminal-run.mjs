/** Terminal, Run/Debug configurations and Run tool window screenshots. */

/** @param {import("playwright-core").Page} page */
async function type(page, text) {
  await page.keyboard.type(text, { delay: 8 });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(120);
}

/** @param {import("playwright-core").Page} page */
async function terminalTabs(page) {
  await page.keyboard.press("Alt+F12");
  await page.waitForSelector(".term-host .xterm-screen");
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

/** @param {import("playwright-core").Page} page */
async function runPhpunit(page) {
  await page.keyboard.press("Shift+F10");
  await page.waitForSelector(".run-tests-status");
  await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("Tests failed"), null, {
    timeout: 10000,
  });
  await page.click('.test-row[data-test="test_display_name_prefers_nickname"]');
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page */
async function toolbarDropdown(page) {
  await page.keyboard.press("Shift+F10");
  await page.waitForSelector(".run-tests-status");
  await page.waitForTimeout(400);
  await page.click(".run-config");
  await page.waitForSelector(".run-dropdown");
  await page.hover('.run-dropdown .run-dd-item[data-config="artisan serve"]');
}

export default {
  "terminal-tabs": terminalTabs,
  "run-configurations-dialog": runConfigurationsDialog,
  "run-phpunit-tree": runPhpunit,
  "run-toolbar-dropdown": toolbarDropdown,
};
