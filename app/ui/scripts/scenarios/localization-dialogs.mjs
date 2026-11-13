/**
 * The translated tool windows and dialogs in German: Git, Debugger, Database, refactorings,
 * Run/Debug Configurations with Run on, the PHP settings pages, and the tool windows of the
 * language packs (.NET Build and NuGet, merge requests, CMake profiles, Python Packages). Each
 * scenario switches the IDE to German and then runs the steps of the English scenario of the
 * feature, so the German screenshots show the same state (and longer German text can be checked
 * for overflow).
 */
import vcs from "./vcs.mjs";
import gitDialogs from "./git-dialogs.mjs";
import debuggerScenarios from "./debugger.mjs";
import databaseConnectivity from "./database-connectivity.mjs";
import moveRefactorings from "./move-refactorings.mjs";
import highlightUsages from "./highlight-usages.mjs";
import settings from "./settings.mjs";
import phpQualityTools from "./php-quality-tools.mjs";
import dotnet from "./dotnetforge.mjs";
import forges from "./forges.mjs";
import cforge from "./cforge.mjs";
import pyforge from "./pyforge.mjs";

/**
 * Store `ide.language = de` in the browser-mode user settings and start again, like after the
 * restart the language setting offers.
 * @param {import("playwright-core").Page} page
 */
async function german(page) {
  await page.evaluate(() => {
    const key = "forge.mock.settings.user";
    const user = JSON.parse(localStorage.getItem(key) ?? "{}");
    localStorage.setItem(key, JSON.stringify({ ...user, "ide.language": "de" }));
  });
  await page.reload();
  await page.waitForSelector(".cm-content");
}

const DS = "shop-localhost";

/** Accept the German ".env" suggestion and wait for the shop schema. @param {import("playwright-core").Page} page */
async function addShop(page) {
  await page.click('.balloon-action:has-text("Datenquelle hinzufügen")');
  await page.waitForSelector(`.db-row[data-key="table:${DS}/shop/users"]`);
}

/** @param {(page: import("playwright-core").Page) => Promise<void>} steps */
const de = (steps) =>
  Object.assign(
    async (page) => {
      await german(page);
      await steps(page);
    },
    // Keep the start page of the English scenario (`?project=dotnet`, …).
    { query: steps.query, waitFor: steps.waitFor },
  );

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "i18n-git-log-de": de(vcs["vcs-log"]),
  "i18n-git-branches-de": de(async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("git.branches"));
    await page.waitForSelector(".branch-row");
    const row = page.locator(".branch-row", { hasText: "main" }).first();
    await row.hover();
    await page.waitForSelector(".branch-submenu");
  }),
  "i18n-git-merge-de": de(gitDialogs["git-merge-dialog"]),
  "i18n-git-reset-de": de(gitDialogs["git-reset-dialog"]),
  "i18n-debugger-de": de(debuggerScenarios["debugger-paused"]),
  "i18n-debugger-breakpoints-de": de(debuggerScenarios["debugger-breakpoints"]),
  "i18n-database-de": de(async (page) => {
    await addShop(page);
    await page.click(`.db-row[data-key="table:${DS}/shop/users"] .tree-chevron`);
    await page.click(`.db-row[data-key="group:${DS}/shop/users/columns"] .tree-chevron`);
    await page.click(`.db-row[data-key="column:${DS}/shop/users/email"]`);
    await page.click(`.db-row[data-key="column:${DS}/shop/users/email"]`, { button: "right" });
    await page.waitForTimeout(200);
  }),
  "i18n-data-source-de": de(async (page) => {
    await addShop(page);
    await page.click(`.db-row[data-key="ds:${DS}"]`);
    await page.evaluate(() => window.__forgeRunCommand("Database.Properties"));
    await page.waitForSelector(".dsd-dialog");
    await page.click(".dsd-test-link");
    await page.waitForSelector(".dsd-test-status.ok");
  }),
  "i18n-data-source-ssh-de": de(databaseConnectivity["database-ssh-ssl"]),
  "i18n-move-class-de": de(moveRefactorings["move-class-dialog"]),
  "i18n-find-usages-de": de(highlightUsages["find-usages-dialog"]),
  "i18n-settings-php-de": de(settings["settings-php"]),
  "i18n-php-quality-tools-de": de(phpQualityTools["php-quality-tools-settings"]),
  "i18n-dotnet-build-de": de(dotnet["dotnet-build"]),
  "i18n-dotnet-nuget-de": de(dotnet["dotnet-nuget"]),
  "i18n-forges-mr-overview-de": de(forges["forges-mr-overview"]),
  "i18n-cmake-profiles-de": de(cforge["cmake-profiles"]),
  "i18n-python-packages-de": de(pyforge["python-packages"]),
  "i18n-run-on-de": de(async (page) => {
    await page.click(".run-config");
    await page.click(".run-dropdown .run-dd-item:has-text('Konfigurationen bearbeiten')");
    await page.waitForSelector(".rc-dialog");
    await page.click('.rc-tree-item[data-config="UserTest"]');
    await page.waitForSelector('[data-testid="rc-run-on"]');
    await page.selectOption('[data-testid="rc-run-on"]', "dockerImage");
    await page.waitForTimeout(200);
  }),
};
