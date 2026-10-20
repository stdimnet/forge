/**
 * Remote development over SSH: Welcome screen > Remote Development, the New SSH Connection
 * dialog, the host key question, the remote folder chooser, a remote project's workbench
 * with the connection status and Settings | Tools | SSH Configurations.
 */

/** Welcome screen > Remote Development > New Connection, filled in for the demo host. */
async function newConnection(page) {
  await page.click("[data-testid=ssh-new]");
  await page.waitForSelector("[data-testid=ssh-dialog]");
  await page.fill("[data-testid=ssh-host]", "shop.example.com");
  await page.fill("[data-testid=ssh-user]", "deploy");
  await page.fill("[data-testid=ssh-password]", "correct horse battery staple");
}

/** Continue to the project step, confirming the unknown host key. */
async function connect(page) {
  await newConnection(page);
  await page.click("[data-testid=ssh-continue]");
  await page.waitForSelector("[data-testid=ssh-hostkey-yes]");
  await page.click("[data-testid=ssh-hostkey-yes]");
  await page.waitForSelector("[data-testid=ssh-project-path]");
}

const remoteWelcome = async () => {};
remoteWelcome.query = "?welcome=remote";
remoteWelcome.waitFor = "[data-testid=welcome-remote] .welcome-project";

const sshDialog = async (page) => {
  await newConnection(page);
  await page.click("[data-testid=ssh-test]");
  await page.waitForSelector("[data-testid=ssh-hostkey-yes]");
  await page.click("[data-testid=ssh-hostkey-yes]");
  await page.waitForSelector("[data-testid=ssh-test-result] .ssh-ok");
};
sshDialog.query = "?welcome=remote";
sshDialog.waitFor = "[data-testid=ssh-new]";

const fingerprint = async (page) => {
  await newConnection(page);
  await page.click("[data-testid=ssh-continue]");
  await page.waitForSelector("[data-testid=ssh-fingerprint]");
};
fingerprint.query = "?welcome=remote";
fingerprint.waitFor = "[data-testid=ssh-new]";

const folderChooser = async (page) => {
  await connect(page);
  await page.click("[data-testid=ssh-browse]");
  await page.waitForSelector("[data-testid=remote-chooser] .ssh-chooser-row");
  await page.fill("[data-testid=remote-chooser-path]", "/var/www");
  await page.press("[data-testid=remote-chooser-path]", "Enter");
  await page.waitForSelector('.ssh-chooser-row[data-path="/var/www/shop"]');
  await page.click('.ssh-chooser-row[data-path="/var/www/shop"]');
};
folderChooser.query = "?welcome=remote";
folderChooser.waitFor = "[data-testid=ssh-new]";

const projectStep = async (page) => {
  await connect(page);
  await page.click("[data-testid=ssh-browse]");
  await page.waitForSelector("[data-testid=remote-chooser] .ssh-chooser-row");
  await page.fill("[data-testid=remote-chooser-path]", "/var/www/shop");
  await page.press("[data-testid=remote-chooser-path]", "Enter");
  await page.waitForSelector('.ssh-chooser-row[data-path="/var/www/shop/app"]');
  await page.click("[data-testid=remote-chooser-ok]");
};
projectStep.query = "?welcome=remote";
projectStep.waitFor = "[data-testid=ssh-new]";

const workbench = async (page) => {
  await page.waitForSelector("[data-testid=remote-status]");
  await page.dblclick('.tree-row[data-path="app"]');
  await page.dblclick('.tree-row[data-path="app/Models"]');
};
workbench.query = "?project=remote";

const statusPopup = async (page) => {
  await page.click("[data-testid=remote-status]");
  await page.waitForSelector(".remote-popup");
};
statusPopup.query = "?project=remote";

const reconnecting = async (page) => {
  await page.evaluate(() => window.__forgeRunCommand("Remote.Reconnect"));
  await page.waitForSelector('[data-testid=remote-status][data-state="reconnecting"]');
};
reconnecting.query = "?project=remote";

const openFromWelcome = async (page) => {
  await page.click('.welcome-project[data-root="deploy@shop.example.com:/var/www/shop"]');
  await page.waitForSelector("[data-testid=ssh-hostkey-yes]");
  await page.click("[data-testid=ssh-hostkey-yes]");
  await page.waitForSelector("[data-testid=remote-status]");
};
openFromWelcome.query = "?welcome";
openFromWelcome.waitFor = ".welcome-project";

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "remote-welcome": remoteWelcome,
  "remote-ssh-dialog": sshDialog,
  "remote-fingerprint": fingerprint,
  "remote-folder-chooser": folderChooser,
  "remote-project-step": projectStep,
  "remote-workbench": workbench,
  "remote-status-popup": statusPopup,
  "remote-reconnecting": reconnecting,
  "remote-open-recent": openFromWelcome,
  "remote-ssh-settings": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("remote.settings"));
    await page.waitForSelector(".ssh-list-row[data-config]");
    await page.click('.ssh-list-row[data-config="shop-key"]');
  },
  "remote-ssh-dialog-light": Object.assign(
    async (page) => {
      await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
      await fingerprint(page);
    },
    { query: "?welcome=remote", waitFor: "[data-testid=ssh-new]" },
  ),
};
