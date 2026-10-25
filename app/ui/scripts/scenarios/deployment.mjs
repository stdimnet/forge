/**
 * Deployment: Settings | Build, Execution, Deployment | Deployment (Connection with Test
 * Connection, Mappings), the Project view's Deployment submenu, the "Upload to" server
 * chooser, the File Transfer tool window after an upload, Sync with Deployed and Compare with
 * Deployed Version.
 */

async function closeBalloons(page) {
  await page.evaluate(() => document.querySelectorAll(".balloon").forEach((b) => b.remove()));
}

async function openDeploymentSettings(page) {
  await closeBalloons(page);
  await page.evaluate(() => window.__forgeRunCommand("PublishGroup.Configuration"));
  await page.waitForSelector(".deploy-settings .ssh-list-row[data-server]");
}

async function projectRowMenu(page, path) {
  const row = page.locator(`.project-tree .tree-row[data-path="${path}"]`);
  await row.click();
  await row.click({ button: "right" });
  await page.waitForSelector(".context-menu");
  await page.hover('.context-menu-item:has-text("Deployment")');
  await page.waitForSelector(".context-menu .context-menu");
}

/** Upload the app folder to the default server, overwriting the newer User.php. */
async function uploadApp(page) {
  await projectRowMenu(page, "app");
  await page.click('.context-menu .context-menu .context-menu-item:has-text("Upload to shop-production")');
  await page.waitForSelector('.confirm-dialog button:has-text("Overwrite"), .dialog button:has-text("Overwrite")');
  await page.click('button:has-text("Overwrite")');
  await page.waitForFunction(() => document.querySelectorAll(".ft-op .ft-ok").length >= 2);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "deployment-settings": async (page) => {
    await openDeploymentSettings(page);
    await page.click('[data-testid="deploy-test"]');
    await page.waitForSelector(".deploy-settings .ssh-test-result .ssh-ok");
  },
  "deployment-mappings": async (page) => {
    await openDeploymentSettings(page);
    await page.click('.deploy-tab:has-text("Mappings")');
    await page.waitForSelector(".deploy-table tbody tr");
  },
  "deployment-options": async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("PublishGroup.Options"));
    await page.waitForSelector(".set-dialog");
    await page.waitForTimeout(300);
  },
  "deployment-upload-menu": async (page) => {
    await closeBalloons(page);
    await projectRowMenu(page, "app");
  },
  "deployment-choose-server": async (page) => {
    // Not awaited: the command waits for the choice.
    await page.evaluate(() => {
      void window.__forgeRunCommand("PublishGroup.UploadTo");
    });
    await page.waitForSelector(".deploy-choose-item");
    await closeBalloons(page);
  },
  "deployment-file-transfer": async (page) => {
    await uploadApp(page);
    await projectRowMenu(page, "public");
    await page.click('.context-menu .context-menu .context-menu-item:has-text("Download from shop-production")');
    await page.waitForFunction(() => document.querySelectorAll(".ft-op").length >= 2 && !document.querySelector(".ft-running"));
    await closeBalloons(page);
  },
  "deployment-sync": async (page) => {
    await closeBalloons(page);
    await page.click('.project-tree .tree-row[data-path="app"]');
    await page.evaluate(() => window.__forgeRunCommand("PublishGroup.SyncLocalVsRemote"));
    await page.waitForSelector(".dir-diff-row");
    await page.click('.dir-diff-filter[title="Show Equal Files"]');
    await page.waitForTimeout(200);
  },
  "deployment-compare": async (page) => {
    await closeBalloons(page);
    await page.click(".cm-content .cm-line:nth-child(3)");
    await page.evaluate(() => window.__forgeRunCommand("PublishGroup.CompareLocalVsRemote"));
    await page.waitForSelector(".diff-viewer, .diff-overlay, [data-focus-context=diff]");
    await page.waitForTimeout(400);
  },
};
