/**
 * CForge screenshots, on the `shapes` CMake demo (`?project=cpp`): the C++ editor with the
 * GoogleTest gutter, the CMake tool window, Settings | CMake profiles and Toolchains, build
 * problems of a failing build (`&build=broken`), the GoogleTest and Catch2 test trees and a
 * GDB debug session with STL containers.
 */

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path, line = 1) {
  await page.evaluate(
    async ([p, l]) => {
      const { openFileAt } = await import("/src/editor/views.ts");
      await openFileAt(p, l, 1, { flash: false });
    },
    [path, line],
  );
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + 200, y);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, 900 - 26 - height, { steps: 4 });
  await page.mouse.up();
}

/** Click the gutter ▶ icon on the line containing `text` and pick a menu item. */
async function gutterRun(page, text, item) {
  await page.waitForSelector(".run-gutter-icon:visible");
  const line = page.locator(".cm-line:visible", { hasText: text }).first();
  const box = await line.boundingBox();
  const icons = page.locator(".cm-run-gutter:visible .cm-gutterElement");
  const count = await icons.count();
  for (let i = 0; i < count; i++) {
    const b = await icons.nth(i).boundingBox();
    if (b && box && Math.abs(b.y - box.y) < 4 && (await icons.nth(i).locator(".run-gutter-icon").count())) {
      await icons.nth(i).locator(".run-gutter-icon").click();
      break;
    }
  }
  if (item) await page.click(`.run-popup-menu .run-menu-item:has-text("${item}")`);
}

/** Run a stored configuration (its target is built first). */
async function run(page, name) {
  await page.evaluate(async (n) => {
    const { runConfiguration } = await import("/src/features/run/runStore.ts");
    const { findConfiguration } = await import("/src/features/run/configStore.ts");
    await runConfiguration(findConfiguration(n));
  }, name);
}

const cpp = (fn) => Object.assign(fn, { query: "?project=cpp" });
const broken = (fn) => Object.assign(fn, { query: "?project=cpp&build=broken" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // GoogleTest tests with ▶ icons: TEST_F, TEST and TEST_P, and the Run / Debug menu.
  "cpp-editor": cpp(async (page) => {
    await closeBalloons(page);
    await openFile(page, "tests/polygon_test.cpp", 14);
    await page.waitForSelector(".run-gutter-icon:visible");
    await gutterRun(page, "TEST_F(PolygonTest, PerimeterOfTriangle)");
    await page.waitForSelector(".run-popup-menu");
    await page.waitForTimeout(300);
  }),
  // The CMake tool window: profiles, targets and the configure log with a warning linked to
  // CMakeLists.txt.
  "cmake-tool-window": cpp(async (page) => {
    await closeBalloons(page);
    await openFile(page, "CMakeLists.txt", 18);
    await page.click('[data-tool-window="cmake"]');
    await page.waitForSelector('[data-testid="cmake-target"]');
    await bottomHeight(page, 360);
    await page.click('[data-target="shapes_app"]');
    await page.waitForTimeout(500);
  }),
  // Settings | Build, Execution, Deployment | CMake: profiles and presets.
  "cmake-profiles": cpp(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="build.cmake"]');
    await page.waitForSelector('[data-testid="cmake-profile-form"]');
    await page.waitForTimeout(600);
  }),
  // Settings | Build, Execution, Deployment | Toolchains.
  "cpp-toolchains": cpp(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="build.toolchains"]');
    await page.waitForSelector('[data-testid="toolchain-details"]');
    await page.waitForTimeout(600);
  }),
  // Build Project (Ctrl+F9) of a broken build: the Problems tool window's Build tab.
  "cpp-build-problems": broken(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/geometry/polygon.cpp", 28);
    await page.evaluate(() => window.__forgeRunCommand("CompileDirty"));
    await page.waitForSelector('[data-testid="cpp-build-problem"]', { timeout: 15000 });
    await bottomHeight(page, 330);
    await page.waitForTimeout(800);
    await closeBalloons(page);
  }),
  // The GoogleTest tree of polygon_tests (built first).
  "cpp-gtest-tree": cpp(async (page) => {
    await closeBalloons(page);
    await openFile(page, "tests/polygon_test.cpp", 30);
    await run(page, "polygon_tests");
    await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("passed"), null, { timeout: 20000 });
    await bottomHeight(page, 340);
    await page.waitForTimeout(600);
  }),
  // Catch2 test cases and sections with a failing section.
  "cpp-catch2-tree": cpp(async (page) => {
    await closeBalloons(page);
    await openFile(page, "tests/stats_test.cpp", 16);
    await run(page, "stats_tests");
    await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("failed"), null, { timeout: 20000 });
    await bottomHeight(page, 340);
    await page.waitForTimeout(600);
  }),
  // GDB stopped in main: std::vector<shapes::Polygon> and its corners, std::vector<double>.
  "cpp-debug": cpp(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/main.cpp", 16);
    const cell = page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: /^16$/ }).first();
    await cell.click();
    await page.waitForSelector(".cm-bp-dot");
    await page.evaluate(async () => {
      const { debugConfiguration } = await import("/src/features/run/runStore.ts");
      const { findConfiguration } = await import("/src/features/run/configStore.ts");
      await debugConfiguration(findConfiguration("shapes_app"));
    });
    await page.waitForSelector(".cm-exec-line", { timeout: 20000 });
    await page.waitForSelector('.dbg-row[data-var="polygons"]', { timeout: 5000 }).catch(() => {});
    await bottomHeight(page, 380);
    await page.click('.dbg-row[data-var="polygons"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(400);
    await page.click('.dbg-row[data-var="[1]"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(400);
    await page.click('.dbg-row[data-var="corners_"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(500);
  }),
  // Settings | Languages & Frameworks | C/C++.
  "cpp-settings": cpp(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.cpp"]');
    await page.waitForSelector('[data-testid="cpp-settings"]');
    await page.waitForTimeout(800);
  }),
};
