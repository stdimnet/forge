/** Git submenu of the Project view context menu, for a modified and an unversioned file. */

/** Right-click `path` in the tree and open the Git submenu. */
async function openGitMenu(page, path, theme) {
  if (theme) await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
  // Let the first git status arrive so the items reflect the file's state.
  await page.waitForSelector(".tree-row.vcs-modified, .tree-row [class*='vcs-']", { timeout: 5000 }).catch(() => {});
  await page.waitForTimeout(300);
  await page.click(`.tree-row[data-path="${path}"]`, { button: "right" });
  await page.waitForSelector(".context-menu");
  await page.hover('.context-menu > .context-menu-item:has(> .context-menu-label:text-is("Git"))');
  await page.waitForSelector(".context-menu .context-menu");
}

async function revealTests(page) {
  await page.click('.tree-row[data-path="tests"]');
  await page.click('.tree-row[data-path="tests/Feature"]');
  await page.waitForSelector('.tree-row[data-path="tests/Feature/OrderTest.php"]');
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "vcs-menu-modified": (page) => openGitMenu(page, "app/Models/User.php"),
  "vcs-menu-modified-light": (page) => openGitMenu(page, "app/Models/User.php", "light"),
  "vcs-menu-unversioned": async (page) => {
    await revealTests(page);
    await openGitMenu(page, "tests/Feature/OrderTest.php");
  },
  "vcs-menu-compare-revision": async (page) => {
    await openGitMenu(page, "app/Models/User.php");
    await page.click('.context-menu .context-menu .context-menu-item:has-text("Compare with Revision")');
    await page.waitForSelector(".git-chooser-row");
  },
  "vcs-menu-unstash": async (page) => {
    await openGitMenu(page, "app/Models/User.php", "light");
    await page.click('.context-menu .context-menu .context-menu-item:has-text("Unstash Changes")');
    await page.waitForSelector(".git-chooser-row");
  },
  "vcs-menu-unversioned-light": async (page) => {
    await revealTests(page);
    await openGitMenu(page, "tests/Feature/OrderTest.php", "light");
  },
};
