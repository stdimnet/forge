/**
 * Icon themes: the SDK's Shapes Icons example (installed in browser mode) as the active icon
 * theme in the Project view, editor tabs and Structure, and the Settings | Appearance &
 * Behavior | File Icons page with a custom association.
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

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
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
    await page.keyboard.press("Control+Alt+KeyS");
    await page.waitForSelector(".set-dialog");
    await page.waitForSelector(".set-row, .plugin-item");
    await page.click('.set-tree-row[data-node="appearance.fileIcons"]');
    await page.waitForSelector(".icon-associations tbody tr");
    await page.waitForTimeout(300);
  },
};
