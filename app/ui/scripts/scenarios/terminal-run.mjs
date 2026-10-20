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

export default {
  "terminal-tabs": terminalTabs,
};
