/**
 * Icon themes: the SDK's Shapes Icons example (installed in browser mode) as the active icon
 * theme in the Project view, editor tabs and Structure, and the Settings | Appearance &
 * Behavior | File Icons page with a custom association. The format 2 scenarios show themed
 * symbols in Structure, UI icons on the tool window stripes and the toolbar, the root folder,
 * light variants, the tint layer with an accent color, plus/minus arrows and hollow folders.
 */

/** @param {import("playwright-core").Page} page @param {string} key @param {unknown} value */
async function setting(page, key, value) {
  await page.evaluate(
    async ([k, v]) => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting(k, v);
    },
    [key, value],
  );
}

/** @param {import("playwright-core").Page} page */
async function useShapes(page) {
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
  await setting(page, "fileIcons.theme", "example.shapes-icons.shapes");
  await page.evaluate(async () => {
    const { showToolWindow } = await import("/src/state/layout.ts");
    showToolWindow("project");
  });
  await page.waitForSelector(".project-tree .themed-file-icon");
}

/** @param {import("playwright-core").Page} page @param {string[]} paths */
async function expand(page, paths) {
  await page.evaluate(async (ps) => {
    const { projectTree } = await import("/src/features/project/ProjectView.tsx");
    for (const p of ps) projectTree.setOpen(p, true);
  }, paths);
  await page.waitForSelector(`.tree-row[data-path^="${paths[paths.length - 1]}/"]`);
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page @param {string[]} paths */
async function open(page, paths) {
  await page.evaluate(async (ps) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    for (const p of ps) await openFileAt(p, 1);
  }, paths);
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function showStructure(page) {
  await page.click('[data-tool-window="structure"]');
  await page.waitForSelector(".themed-symbol");
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "icon-themes-structure": async (page) => {
    await useShapes(page);
    await expand(page, ["app", "app/Http", "app/Http/Controllers", "app/Models"]);
    await showStructure(page);
    await page.waitForSelector('.tool-stripe [data-ui-icon="toolwindow.project"]');
  },
  "icon-themes-light": async (page) => {
    await useShapes(page);
    await page.evaluate(async () => {
      const { setTheme } = await import("/src/state/theme.ts");
      setTheme("light");
    });
    await expand(page, ["app", "app/Models", "config"]);
    await open(page, ["composer.json"]);
  },
  "icon-themes-tint": async (page) => {
    await useShapes(page);
    await setting(page, "fileIcons.accentColor", "#3d8bfd");
    await expand(page, ["app", "app/Models", "config", "public", "tests"]);
    await page.waitForTimeout(300);
  },
  "icon-themes-plus-minus": async (page) => {
    await useShapes(page);
    await setting(page, "fileIcons.treeArrows", "plusMinus");
    await expand(page, ["app", "app/Models", "config", "resources"]);
    await page.waitForSelector(".project-tree .tree-plus-minus");
  },
  "icon-themes-hollow": async (page) => {
    await useShapes(page);
    await setting(page, "fileIcons.hollowFolders", "on");
    await expand(page, ["app", "app/Http", "app/Models", "config", "routes"]);
    await open(page, ["app/Models/User.php", "config/app.php"]);
  },
  "icon-theme-project-view": async (page) => {
    await useShapes(page);
    await expand(page, ["app", "app/Models", "config", "resources", "resources/views", "tests", "tests/Unit"]);
    await page.evaluate(async () => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt("resources/views/welcome.blade.php", 1);
      await openFileAt("composer.json", 1);
    });
    await page.waitForTimeout(500);
  },
  "settings-file-icons": async (page) => {
    await useShapes(page);
    await setting(page, "fileIcons.associations", [
      { kind: "fileName", value: "artisan", icon: "php" },
      { kind: "folderName", value: "database", icon: "folder-config" },
    ]);
    await setting(page, "fileIcons.folderColors", [{ pattern: "vendor", color: "#8a8f99" }]);
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.waitForSelector(".set-row, .plugin-item");
    await page.click('.set-tree-row[data-node="appearance.fileIcons"]');
    await page.waitForSelector(".icon-associations tbody tr");
    await page.waitForTimeout(300);
  },
};
