/** New Project wizard, project type detection and Welcome screen badges. */

const openWizard = async (page, generator) => {
  await page.evaluate(() => window.__forgeRunCommand("NewProject"));
  await page.waitForSelector(".np-dialog .np-item");
  if (generator) await page.click(`.np-item[data-generator="${generator}"]`);
  // The command preview is planned asynchronously.
  await page.waitForSelector(".np-preview-body div:nth-child(2)");
  await page.waitForTimeout(250);
};

const welcomeBadges = async () => {};
welcomeBadges.query = "?welcome";
welcomeBadges.waitFor = ".welcome-project .pt-icon";

const wizardLaravel = async (page) => {
  await openWizard(page, "laravel");
  await page.click('.np-segmented button:has-text("React")');
  await page.click('.np-check:has-text("Create Git repository") input');
  await page.waitForTimeout(300);
};
wizardLaravel.query = "?welcome";
wizardLaravel.waitFor = ".welcome-project";

const wizardLove = async (page) => {
  await openWizard(page, "love-game");
  await page.fill(".np-row:has-text('Name:') input", "pixel-dungeon");
};
wizardLove.query = "?welcome";
wizardLove.waitFor = ".welcome-project";

const wizardMissingTool = async (page) => {
  await openWizard(page, "react");
  await page.waitForSelector(".np-warning");
};
wizardMissingTool.query = "?welcome&missing=npm";
wizardMissingTool.waitFor = ".welcome-project";

const wizardProgress = async (page) => {
  await openWizard(page, "laravel");
  await page.click(".np-footer .btn-primary");
  await page.waitForSelector(".np-log div:nth-child(14)");
  await page.waitForTimeout(150);
};
wizardProgress.query = "?welcome";
wizardProgress.waitFor = ".welcome-project";

const detectionBalloon = async (page) => {
  await page.waitForSelector(".balloon.interactive");
  await page.waitForSelector(".status-project-type");
};
detectionBalloon.query = "?announce";

const wizardLight = async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await openWizard(page, "symfony");
};
wizardLight.query = "?welcome";
wizardLight.waitFor = ".welcome-project";

const defaultProjectDirectory = async (page) => {
  await page.evaluate(() => window.__forgeRunCommand("settings.open"));
  await page.click('.set-tree-row[data-node="system"]');
  await page.waitForSelector("text=Default project directory");
};

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "welcome-project-badges": welcomeBadges,
  "new-project-laravel": wizardLaravel,
  "new-project-love": wizardLove,
  "new-project-missing-tool": wizardMissingTool,
  "new-project-progress": wizardProgress,
  "new-project-light": wizardLight,
  "project-detected-balloon": detectionBalloon,
  "settings-default-project-directory": defaultProjectDirectory,
};
