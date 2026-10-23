/** Invalidate Caches, Repair IDE, Manage IDE Settings, Settings for New Projects and File Types. */

/** @param {import("playwright-core").Page} page @param {string} id */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/** Hover a row of the innermost open menu. @param {import("playwright-core").Page} page @param {string} label */
async function hover(page, label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".context-menu-label", { hasText: exact }).last().hover();
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page @param {string} pageId */
async function openSettingsPage(page, pageId) {
  await page.evaluate(async (id) => {
    const { openSettings } = await import("/src/features/settings/dialogState.ts");
    openSettings(id);
  }, pageId);
  await page.waitForSelector(".set-dialog");
}

/** @type {Record<string, ((page: import("playwright-core").Page) => Promise<void>) & { query?: string, waitFor?: string }>} */
const scenarios = {
  "repair-ide-file-menu": async (page) => {
    await page.click("[data-main-menu-button]");
    await page.waitForSelector(".context-menu.main-menu-popup");
    await hover(page, "File");
    await hover(page, "Manage IDE Settings");
  },
  "invalidate-caches": async (page) => {
    await run(page, "InvalidateCaches");
    await page.waitForSelector('.mnt-dialog [data-option="fileSystemAndLocalHistory"]');
    await page.click('[data-option="fileSystemAndLocalHistory"] input');
    await page.click('[data-option="sharedIndexes"] input');
  },
  "repair-ide-balloon": async (page) => {
    await run(page, "CallSaul");
    await page.waitForSelector('.balloon:has-text("Repair IDE: step 1")');
  },
  "export-settings": async (page) => {
    await page.evaluate(async () => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting("editor.font.size", 15);
      await saveSetting("keymap.active", "macOS");
      await saveSetting("editor.tabs.limit", 12);
    });
    await run(page, "ExportSettings");
    await page.waitForSelector('.mnt-dialog [data-group="editor"]');
  },
  "settings-for-new-projects": async (page) => {
    await run(page, "TemplateProjectProperties");
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.php"]');
    await page.waitForSelector(".set-scope");
  },
  "file-types-settings": async (page) => {
    await page.evaluate(async () => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting("fileTypes.associations", [
        { fileType: "php", pattern: "*.module" },
        { fileType: "php", pattern: "*.phtml", action: "remove" },
      ]);
      await saveSetting("fileTypes.custom", [
        { id: "custom-1", name: "Build Script", description: "Deploy pipeline DSL", lineComment: "#", keywords1: "stage step run\nwhen" },
      ]);
    });
    await openSettingsPage(page, "editor.file-types");
    await page.waitForSelector('.ft-type[data-type="php"]');
    await page.click('.ft-type[data-type="php"]');
    await page.click('[data-testid="ft-add-pattern"]');
    await page.fill(".ft-line-form input", "*.tpl");
  },
  "file-types-custom": async (page) => {
    await openSettingsPage(page, "editor.file-types");
    await page.click('[data-testid="ft-add-type"]');
    await page.fill('[data-testid="ft-name"]', "Build Script");
    await page.fill('[data-testid="ft-keywords"]', "stage\nstep\nrun\nwhen");
  },
  "custom-file-type-editor": async (page) => {
    await page.evaluate(async () => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting("fileTypes.custom", [
        {
          id: "custom-1",
          name: "Build Script",
          lineComment: "#",
          blockCommentStart: "/*",
          blockCommentEnd: "*/",
          hexPrefix: "0x",
          keywords1: "stage step run when",
          keywords2: "echo deploy",
          keywords3: "string int",
          keywords4: "true false",
        },
      ]);
      await saveSetting("fileTypes.associations", [{ fileType: "custom-1", pattern: "*.build" }]);
      await window.__forgeMockWriteFile?.(
        "deploy.build",
        [
          "/* Release pipeline */",
          "stage build {",
          "  step compile when branch == \"main\" # only main",
          "  run echo \"Building 0xFF\" 42",
          "}",
          "stage deploy {",
          "  run deploy --retries 3 true",
          "}",
          "",
        ].join("\n"),
      );
      const { openFile } = await import("/src/state/editor.ts");
      await openFile("deploy.build");
    });
    await page.waitForSelector('.editor-tab:has-text("deploy.build"), .cm-content');
    await page.waitForTimeout(800);
  },
  "invalidate-caches-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await run(page, "InvalidateCaches");
    await page.waitForSelector('.mnt-dialog [data-option="browserEngine"]');
    await page.click('[data-option="browserEngine"] input');
  },
  "welcome-manage-settings": async (page) => {
    await page.click('[data-testid="welcome-manage"]');
    await page.waitForSelector(".context-menu");
  },
};

scenarios["welcome-manage-settings"].query = "?welcome";
scenarios["welcome-manage-settings"].waitFor = ".welcome-nav";

export default scenarios;
