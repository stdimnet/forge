/**
 * Git Branches popup: Update on a local branch that is not checked out, Pull into the current
 * branch on a remote branch; the compact Git tool window (tabs in the header); and branches
 * grouped into folders by prefix (popup and the Log tab's branches tree), dark and light.
 */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

async function submenuOf(page, branch) {
  await run(page, "git.branches");
  await page.waitForSelector(".branches-popup .branch-row");
  await page.locator(`.branches-popup .branch-row[data-branch="${branch}"]`).last().hover();
  await page.waitForSelector(".branch-submenu");
  await page.waitForTimeout(200);
}

const light = (page) => page.evaluate(() => (document.documentElement.dataset.theme = "light"));

/** The popup with nested folders open under Local (feature > ui) and under a remote (origin > feature > ui). */
async function popupFolders(page) {
  await run(page, "git.branches");
  await page.waitForSelector(".branches-popup .branch-folder");
  await page.click('.branch-folder[data-folder="feature/ui"]');
  await page.click('.branch-folder[data-folder="origin/feature"]');
  await page.click('.branch-folder[data-folder="origin/feature/ui"]');
  await page.waitForSelector('.branch-row[data-branch="origin/feature/ui/dark-mode"]');
  // The keyboard selection on a closed folder.
  await page.mouse.move(900, 500);
  for (let i = 0; i < 30 && !(await page.$('.branch-folder.active[data-folder="bugfix"]')); i++) await page.keyboard.press("ArrowUp");
  await page.waitForTimeout(200);
}

/** The popup while searching: matches highlighted, in their folders. */
async function popupSearch(page, text, waitFor) {
  await run(page, "git.branches");
  await page.waitForSelector(".branches-popup .branch-folder");
  await page.keyboard.type(text);
  await page.waitForSelector(waitFor);
  await page.waitForTimeout(200);
}

/** The Log tab's branches tree with nested folders open. */
async function logFolders(page) {
  await run(page, "git.log");
  await page.waitForSelector(".log-row");
  // Give the bottom panel more room, so the whole tree shows.
  await page.mouse.move(700, 900 - 26 - 241);
  await page.mouse.down();
  await page.mouse.move(700, 330, { steps: 4 });
  await page.mouse.up();
  await page.click(".log-branches [data-folder='local:dir:feature/ui']");
  await page.click(".log-branches [data-folder='remote:dir:origin/feature']");
  await page.click(".log-branches [data-folder='remote:dir:origin/feature/ui']");
  await page.click(".log-branches [data-branch='origin/feature/ui/dark-mode']");
  await page.mouse.move(900, 200);
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "git-branch-update": async (page) => {
    await submenuOf(page, "main");
    await page.waitForSelector(".branch-submenu .popup-label:text-is('Update')");
  },
  "git-pull-into": async (page) => {
    await submenuOf(page, "origin/main");
    await page.waitForSelector(".branch-submenu .popup-row:has-text('Using Merge')");
  },
  "git-tool-window-compact": async (page) => {
    await run(page, "git.log");
    await page.waitForSelector(".log-row");
    await page.waitForTimeout(300);
  },
  "git-tool-window-compact-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await run(page, "git.log");
    await page.waitForSelector(".log-row");
    await page.waitForTimeout(300);
  },
  "git-branches-folders": popupFolders,
  "git-branches-folders-light": async (page) => {
    await light(page);
    await popupFolders(page);
  },
  "git-branches-folders-search": (page) => popupSearch(page, "dark", '.branch-row[data-branch="origin/feature/ui/dark-mode"] mark'),
  "git-branches-folders-search-light": async (page) => {
    await light(page);
    await popupSearch(page, "dark", '.branch-row[data-branch="origin/feature/ui/dark-mode"] mark');
  },
  // A folder whose own name matches is shown closed, with the match highlighted.
  "git-branches-folders-search-folder": (page) => popupSearch(page, "ui", '.branch-folder[data-folder="origin/feature/ui"] mark'),
  "git-branches-folders-search-folder-light": async (page) => {
    await light(page);
    await popupSearch(page, "ui", '.branch-folder[data-folder="origin/feature/ui"] mark');
  },
  "git-branches-settings": async (page) => {
    await run(page, "git.branches");
    await page.waitForSelector(".branches-popup .branch-folder");
    await page.click('.branches-popup [aria-haspopup="menu"]');
    await page.waitForSelector(".branch-settings-menu");
    await page.waitForTimeout(200);
  },
  "git-log-branch-folders": logFolders,
  "git-log-branch-folders-light": async (page) => {
    await light(page);
    await logFolders(page);
  },
};

export default scenarios;
