/**
 * Minecraft plugins: the Minecraft Plugin New Project wizard (after Refresh versions, with a
 * Kotlin variant that hides the Maven-only options), and the generated Paper plugin
 * (`?project=minecraft`): event handler gutter icons in a listener and plugin.yml with an
 * unresolved main class.
 */

const openWizard = async (page) => {
  await page.evaluate(() => window.__forgeRunCommand("NewProject"));
  await page.waitForSelector(".np-dialog .np-item");
  await page.click('.np-item[data-generator="minecraft-plugin"]');
  // The file preview is planned asynchronously (WebAssembly in browser mode).
  await page.waitForSelector(".np-preview-file");
  await page.waitForTimeout(250);
};

const minecraft = (fn) => Object.assign(fn, { query: "?project=minecraft", waitFor: ".tree-row" });

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path, line = 1) {
  await page.evaluate(
    async ([p, l]) => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt(p, l, 1);
    },
    [path, line],
  );
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

const wizard = async (page) => {
  await openWizard(page);
  await page.fill(".np-row:has-text('Name:') input", "better-chat");
  await page.fill('input[data-option="authors"]', "Alex, Sam");
  await page.fill('input[data-option="description"]', "Nicer chat messages");
  await page.click('button[aria-label="Refresh Minecraft"]');
  await page.waitForFunction(() =>
    [...document.querySelectorAll('select[data-option="mcVersion"] option')].some((o) => o.value === "26.1"),
  );
  await page.evaluate(() => {
    const main = document.querySelector(".np-main");
    if (main) main.scrollTop = 0;
  });
  await page.waitForTimeout(400);
};
wizard.query = "?welcome";
wizard.waitFor = ".welcome-project";

const wizardKotlin = async (page) => {
  await openWizard(page);
  await page.selectOption('select[data-option="platform"]', "purpur");
  await page.click('.np-segmented button:text-is("Kotlin")');
  await page.selectOption('select[data-option="descriptor"]', "paper-plugin");
  await page.waitForTimeout(500);
  // Show the file preview at the bottom of the form.
  await page.evaluate(() => {
    const main = document.querySelector(".np-main");
    if (main) main.scrollTop = main.scrollHeight;
  });
  await page.waitForTimeout(300);
};
wizardKotlin.query = "?welcome";
wizardKotlin.waitFor = ".welcome-project";

const wizardVelocityLight = async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await openWizard(page);
  await page.selectOption('select[data-option="platform"]', "velocity");
  await page.waitForTimeout(500);
};
wizardVelocityLight.query = "?welcome";
wizardVelocityLight.waitFor = ".welcome-project";

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "minecraft-wizard": wizard,
  "minecraft-wizard-kotlin": wizardKotlin,
  "minecraft-wizard-velocity-light": wizardVelocityLight,
  "minecraft-project": minecraft(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/main/java/com/example/betterchat/listener/ChatListener.java", 14);
    await page.locator(".cm-line-marker.marker-minecraft-listener").first().waitFor({ timeout: 15000 });
    await page.dblclick('.tree-row[data-path="src"]').catch(() => {});
    await page.waitForTimeout(600);
  }),
  "minecraft-plugin-yml": minecraft(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/main/resources/plugin.yml", 3);
    await page.evaluate(async () => {
      const { activeView } = await import("/src/editor/views.ts").catch(() => ({}));
      const view = typeof activeView === "function" ? activeView() : null;
      if (!view) return;
      const text = view.state.doc.toString();
      const from = text.indexOf("BetterChat\napi-version");
      if (from >= 0) view.dispatch({ changes: { from, to: from + "BetterChat".length, insert: "BetterChatPlugin" } });
    });
    await page.waitForSelector(".cm-lintRange-error, .cm-diagnostic-error, .cm-lint-marker-error", { timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(800);
  }),
};
