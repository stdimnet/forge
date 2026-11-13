/**
 * Remote Host and SSH sessions: the Remote Host tool window (Tools | Deployment | Browse
 * Remote Host) with mapped folders, its context menu, an Edit Remote File tab, Settings |
 * Tools | SSH Configurations with "Parse config file", the Start SSH Session chooser and an
 * SSH session in the Terminal.
 */

async function closeBalloons(page) {
  await page.evaluate(() => document.querySelectorAll(".balloon").forEach((b) => b.remove()));
}

async function browse(page) {
  await closeBalloons(page);
  await page.evaluate(() => window.__forgeRunCommand("PublishGroup.BrowseServers"));
  await page.waitForSelector('[data-testid="remote-host-tree"] .tree-row[data-remote-path="public"]');
  await page.dblclick('.rh-row[data-remote-path="public"]');
  await page.waitForSelector('.rh-row[data-remote-path="public/maintenance.html"]');
  await page.dblclick('.rh-row[data-remote-path="app"]');
  await page.waitForSelector('.rh-row[data-remote-path="app/Models"]');
  await page.dblclick('.rh-row[data-remote-path="app/Models"]');
  await page.waitForSelector('.rh-row[data-remote-path="app/Models/User.php"]');
}

async function chooser(page) {
  await closeBalloons(page);
  // Not awaited: the command waits for the choice.
  await page.evaluate(() => {
    void window.__forgeRunCommand("RunSshConsole");
  });
  await page.waitForSelector(".term-ssh-chooser .term-ssh-item");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "remote-host-tree": async (page) => {
    await browse(page);
    await page.click('.rh-row[data-remote-path="app/Models/User.php"]');
  },
  "remote-host-context-menu": async (page) => {
    await browse(page);
    const row = page.locator('.rh-row[data-remote-path="public/maintenance.html"]');
    await row.click();
    await row.click({ button: "right" });
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu-item:has-text("New")');
    await page.waitForTimeout(200);
  },
  "remote-host-edit": async (page) => {
    await browse(page);
    await page.dblclick('.rh-row[data-remote-path="app/Models/User.php"]');
    await page.waitForFunction(() => document.querySelector(".cm-content")?.textContent?.includes("class User"));
    await page.waitForTimeout(300);
  },
  "remote-host-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await browse(page);
    await page.click('.rh-row[data-remote-path="public"]');
  },
  "ssh-configurations": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("remote.settings"));
    await page.waitForSelector(".ssh-list-row[data-config]");
    await page.click('.ssh-list-row[data-config="shop-key"]');
    await page.waitForSelector('[data-testid="ssh-parse-config"]');
  },
  "ssh-session-chooser": chooser,
  "terminal-ssh-dropdown": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("terminal.open"));
    await page.waitForSelector(".term-host .xterm-screen");
    await page.click('.term-new[title="New Tab Options"]');
    await page.waitForSelector('.context-menu :text("SSH Session…")');
  },
  "ssh-session-terminal": async (page) => {
    await chooser(page);
    await page.click('[data-ssh-choice="server:shop-production"]');
    await page.waitForSelector('[data-terminal-tab="shop.example.com"]');
    await page.waitForSelector(".term-host:not(.hidden) .xterm-screen");
    await page.waitForTimeout(800);
    await page.keyboard.type("uptime");
    await page.waitForTimeout(400);
  },
};
