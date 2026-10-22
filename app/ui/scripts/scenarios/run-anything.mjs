/** Run Anything (double Ctrl), compound run configurations and Before launch tasks. */

/** @param {import("playwright-core").Page} page */
async function openRunAnything(page) {
  await page.click(".cm-content");
  // Double Ctrl, through the keymap.
  await page.keyboard.press("Control");
  await page.keyboard.press("Control");
  await page.waitForSelector(".ra-popup .se-row");
}

/** @param {import("playwright-core").Page} page */
async function openConfigurations(page) {
  await page.evaluate(() => window.__forgeRunCommand("run.editConfigurations"));
  await page.waitForSelector(".rc-tree");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "run-anything": async (page) => {
    await openRunAnything(page);
  },
  "run-anything-query": async (page) => {
    await openRunAnything(page);
    await page.keyboard.type("php art");
    await page.waitForSelector('.ra-popup .se-group:has-text("Artisan commands")');
  },
  "run-anything-help": async (page) => {
    await openRunAnything(page);
    await page.keyboard.type("?");
    await page.waitForSelector('.ra-popup .se-group:has-text("Help")');
  },
  "run-anything-debug": async (page) => {
    await openRunAnything(page);
    await page.keyboard.type("npm");
    await page.waitForSelector('.ra-popup .se-group:has-text("npm scripts")');
    await page.keyboard.down("Shift");
    await page.waitForSelector(".ra-popup.debug");
  },
  "run-anything-ran": async (page) => {
    await openRunAnything(page);
    await page.keyboard.type("composer test");
    await page.waitForSelector('.ra-popup .se-row.selected:has-text("composer test")');
    await page.keyboard.press("Enter");
    await page.waitForSelector('.run-tab:has-text("composer test")');
    await page.waitForTimeout(800);
  },
  "run-configurations-compound": async (page) => {
    await openConfigurations(page);
    await page.click('.rc-toolbar button[title^="Add New Configuration"]');
    await page.click('.run-menu-item:has-text("Compound")');
    await page.fill(".rc-name-input", "Full stack");
    for (const name of ["artisan serve", "dev"]) {
      await page.click('[data-testid="rc-compound"] button[title="Add"]');
      await page.click(`.run-menu-item:has-text("${name}")`);
    }
    await page.click(".rc-compound-multiple input");
  },
  "run-configurations-before-launch": async (page) => {
    await openConfigurations(page);
    await page.click('.rc-tree-item[data-config="artisan serve"]');
    const add = '[data-testid="rc-before-launch"] button[title="Add"]';
    await page.click(add);
    await page.click('.run-menu-item:has-text("Run npm script")');
    await page.keyboard.type("build");
    await page.click(add);
    await page.click('.run-menu-item:has-text("Run Another Configuration")');
    await page.click('.run-menu-item:has-text("artisan migrate")');
    await page.click(add);
    await page.click('.run-menu-item:has-text("Run External tool")');
    await page.click('.run-menu-item:has-text("PHPStan")');
    await page.click(add);
    await page.click('.run-menu-item:has-text("Launch Web Browser")');
    await page.keyboard.type("http://127.0.0.1:8000");
  },
};
