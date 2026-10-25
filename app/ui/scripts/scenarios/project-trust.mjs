/**
 * Project trust: "Trust and Open Project?" for a folder outside the trusted `~/Projects`,
 * the Safe Mode banner after "Preview in Safe Mode", "Trust Project?" from the banner and
 * Settings | Build, Execution, Deployment | Trusted Locations.
 */

const UNTRUSTED = "~/Downloads/legacy-crm";

/** Start opening an untrusted folder; the dialog stays open. @param {import("playwright-core").Page} page */
async function askToOpen(page) {
  await page.evaluate(async (path) => {
    const { openWorkspace } = await import("/src/state/workspace.ts");
    void openWorkspace(path);
  }, UNTRUSTED);
  await page.waitForSelector('[data-testid="trust-dialog"]');
  await page.waitForTimeout(150);
}

/** Open the untrusted folder in Safe Mode. @param {import("playwright-core").Page} page */
async function openInSafeMode(page) {
  await askToOpen(page);
  await page.click('[data-testid="trust-safe-mode"]');
  await page.waitForSelector('[data-testid="safe-mode-banner"]');
  await page.evaluate(async () => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt("app/Models/User.php", 1);
  });
  await page.waitForSelector(".cm-content");
  await page.waitForTimeout(250);
}

/** @param {import("playwright-core").Page} page @param {string} pageId */
async function openSettingsPage(page, pageId) {
  await page.evaluate(async (id) => {
    const { openSettings } = await import("/src/features/settings/dialogState.ts");
    openSettings(id);
  }, pageId);
  await page.waitForSelector(".set-dialog");
  await page.waitForSelector('[data-testid="trusted-locations"]');
  await page.waitForTimeout(150);
}

/** @type {Record<string, ((page: import("playwright-core").Page) => Promise<void>) & { query?: string, waitFor?: string }>} */
const scenarios = {
  "trust-open-project": async (page) => {
    await askToOpen(page);
    await page.click('[data-testid="trust-location"]');
  },
  "safe-mode-banner": openInSafeMode,
  "safe-mode-run-refused": async (page) => {
    await openInSafeMode(page);
    await page.evaluate(async () => {
      const { runApi } = await import("/src/ipc/run.ts");
      const { showBalloon } = await import("/src/keymap/index.ts");
      try {
        await runApi.start({ command: { program: "composer", args: ["run", "post-install-cmd"] } });
      } catch (e) {
        showBalloon("Cannot run the configuration", String(e).replace(/^Error: /, ""), 60000, undefined, { type: "error" });
      }
    });
    await page.waitForSelector(".balloon");
    await page.waitForTimeout(200);
  },
  "safe-mode-trust-dialog": async (page) => {
    await openInSafeMode(page);
    await page.click('[data-testid="safe-mode-trust"]');
    await page.waitForSelector('[data-testid="trust-dialog"]');
    await page.waitForTimeout(150);
  },
  "trusted-locations": async (page) => {
    await openSettingsPage(page, "build.trusted-locations");
    await page.click('[data-testid="trust-add"]');
    await page.fill('[data-testid="trust-add-path"]', "~/Work/clients");
    await page.keyboard.press("Enter");
    await page.waitForSelector('tr[data-entry="~/Work/clients"]');
    await page.click('tr[data-entry="~/Projects"]');
    await page.waitForTimeout(150);
  },
  "trust-open-project-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await askToOpen(page);
  },
};

export default scenarios;
