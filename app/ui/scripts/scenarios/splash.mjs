/**
 * Startup splash and updates. `?splash` previews the splash window with the mocked loading
 * steps; `?splashPause=<percent>` holds the steps there; `?simulateUpdate=0.2.0` offers a
 * simulated update.
 */

/** @param {import("playwright-core").Page} page */
const settle = (page, ms = 1800) => page.waitForTimeout(ms);

const splashProgress = async (page) => {
  await page.waitForSelector('[data-testid="splash-status"]:has-text("Opening project")');
  await settle(page);
};
splashProgress.query = "?splash&splashPause=40";
splashProgress.waitFor = ".splash-word";

const splashLight = async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await page.waitForSelector('[data-testid="splash-status"]:has-text("Opening project")');
  await settle(page);
};
splashLight.query = "?splash&splashPause=40";
splashLight.waitFor = ".splash-word";

const splashUpdate = async (page) => {
  await page.waitForSelector(".splash-update.show");
  await settle(page, 900);
};
splashUpdate.query = "?splash&simulateUpdate=0.2.0";
splashUpdate.waitFor = ".splash-word";

const splashDownloading = async (page) => {
  await page.waitForSelector(".splash-update.show");
  await settle(page, 700);
  await page.click('.splash-update button:has-text("Download and restart")');
  await page.waitForSelector('[data-testid="update-download"]');
  await settle(page, 800);
};
splashDownloading.query = "?splash&simulateUpdate=0.2.0";
splashDownloading.waitFor = ".splash-word";

const splashVerified = async (page) => {
  await page.waitForSelector(".splash-update.show");
  await page.click('.splash-update button:has-text("Download and restart")');
  await page.waitForSelector('.splash-update .upd-ok:has-text("Signature: valid")');
  await settle(page, 300);
};
splashVerified.query = "?splash&simulateUpdate=0.2.0";
splashVerified.waitFor = ".splash-word";

const checkForUpdates = async (page) => {
  await page.evaluate(() => window.__forgeRunCommand("CheckForUpdate"));
  await page.waitForSelector('[data-testid="update-dialog"] [data-testid="update-panel"]');
  await settle(page, 500);
};
checkForUpdates.query = "?simulateUpdate=0.2.0";

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "splash-progress": splashProgress,
  "splash-light": splashLight,
  "splash-update": splashUpdate,
  "splash-downloading": splashDownloading,
  "splash-verified": splashVerified,
  "check-for-updates": checkForUpdates,
  "settings-updates": async (page) => {
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.fill(".set-sidebar .set-search input", "updates");
    await page.waitForTimeout(150);
    await page.click('.set-tree-row[data-node="system.updates"]');
    await page.waitForSelector('[data-testid="updates-status"]');
    await page.fill(".set-sidebar .set-search input", "");
    await page.waitForTimeout(200);
  },
};
