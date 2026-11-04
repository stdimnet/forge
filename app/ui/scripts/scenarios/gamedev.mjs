/**
 * DotNetForge for game engines. Unity (`?project=unity`): the Unity tool window connected to
 * the simulated editor with its console after Play, a ShaderLab shader with HLSL, Attach to
 * Unity Process, Settings | .NET | Unity, and Unity tests in batch mode
 * (`&editor=closed`). Godot (`?project=godot`): GDScript, a scene, the Godot run
 * configuration, GdUnit4 results and Settings | .NET | Godot.
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
      await openFileAt(p, l, 1);
    },
    [path, line],
  );
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(500);
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

/** @param {import("playwright-core").Page} page */
async function runConfiguration(page, name) {
  await page.evaluate(async (n) => {
    const { launch } = await import("/src/features/run/runStore.ts");
    const { findConfiguration } = await import("/src/features/run/configStore.ts");
    await launch(findConfiguration(n));
  }, name);
}

/** @param {import("playwright-core").Page} page */
async function settingsPage(page, node, testId) {
  await page.evaluate(() => window.__forgeRunCommand("settings.open"));
  await page.waitForSelector(".set-dialog");
  await page.click('.set-tree-row[data-node="lang.dotnet"]');
  await page.click(`.set-tree-row[data-node="${node}"]`);
  await page.waitForSelector(`[data-testid="${testId}"]`);
  await page.waitForTimeout(800);
}

const unity = (fn, extra = "") => Object.assign(fn, { query: `?project=unity${extra}`, waitFor: ".tree-row" });
const godot = (fn, extra = "") => Object.assign(fn, { query: `?project=godot${extra}`, waitFor: ".tree-row" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // The Unity tool window: connected to Unity, Play streamed the console; an exception selected.
  "unity-console": unity(async (page) => {
    await openFile(page, "Assets/Scripts/Enemy.cs", 17);
    await page.click('[data-tool-window="unity"]');
    await page.waitForSelector('[data-testid="unity-connected"]', { timeout: 15000 });
    await closeBalloons(page);
    await bottomHeight(page, 380);
    await page.click('[data-testid="unity-play"]');
    await page.waitForSelector(".gd-log-row.gd-error", { timeout: 10000 });
    await page.waitForTimeout(1200);
    await page.click(".gd-log-row.gd-error");
    await page.waitForSelector('[data-testid="unity-log-detail"]');
    await page.waitForTimeout(400);
  }),
  // A ShaderLab shader with its HLSL program.
  "unity-shader": unity(async (page) => {
    await closeBalloons(page);
    await openFile(page, "Assets/Shaders/Toon.shader", 1);
    await closeBalloons(page);
  }),
  // The Unity tool window of a project without the editor bridge.
  "unity-bridge-install": unity(async (page) => {
    await page.click('[data-tool-window="unity"]');
    await page.waitForSelector('[data-testid="unity-intro"]');
    await closeBalloons(page);
    await page.click('[data-testid="unity-install-bridge"]');
    await page.waitForTimeout(600);
  }, "&bridge=none&editor=closed"),
  // Run | Attach to Unity Process…
  "unity-attach": unity(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("unity.attach"));
    await page.waitForSelector('[data-testid="unity-attach"] .gd-attach-row');
    await page.waitForTimeout(400);
  }),
  // Unity tests in batch mode with NUnit results (Unity is closed).
  "unity-tests": unity(async (page) => {
    await openFile(page, "Assets/Tests/EditMode/PlayerTests.cs", 30);
    await closeBalloons(page);
    await runConfiguration(page, "Edit Mode Tests");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => /failed/.test(document.querySelector(".run-tests-status")?.textContent ?? ""), null, { timeout: 20000 });
    await page.waitForTimeout(800);
    await bottomHeight(page, 380);
    await page.click('.test-row[data-test="Health_Decreases"]').catch(() => {});
    await page.waitForTimeout(500);
  }, "&editor=closed"),
  // Settings | Languages & Frameworks | .NET | Unity.
  "unity-settings": unity(async (page) => {
    await closeBalloons(page);
    await settingsPage(page, "lang.dotnet.unity", "unity-settings-info");
  }),
  // GDScript highlighting with code insight from the Godot editor.
  "godot-gdscript": godot(async (page) => {
    await closeBalloons(page);
    await openFile(page, "player.gd", 1);
    await closeBalloons(page);
  }),
  // A scene file.
  "godot-scene": godot(async (page) => {
    await closeBalloons(page);
    await openFile(page, "main.tscn", 1);
  }),
  // Run | Edit Configurations: the Godot type.
  "godot-run-config": godot(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openRunConfigurations } = await import("/src/features/run/dialogState.ts");
      openRunConfigurations("player");
    });
    await page.waitForTimeout(1200);
  }),
  // GdUnit4 results.
  "godot-gdunit": godot(async (page) => {
    await openFile(page, "test/player_test.gd", 1);
    await closeBalloons(page);
    await runConfiguration(page, "GdUnit4 Tests");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => /failed/.test(document.querySelector(".run-tests-status")?.textContent ?? ""), null, { timeout: 20000 });
    await page.waitForTimeout(800);
    await bottomHeight(page, 380);
    await page.click('.test-row[data-test="test_speed_default"]').catch(() => {});
    await page.waitForTimeout(500);
  }),
  // Settings | Languages & Frameworks | .NET | Godot.
  "godot-settings": godot(async (page) => {
    await closeBalloons(page);
    await settingsPage(page, "lang.dotnet.godot", "godot-settings-info");
  }),
  // Without the Godot editor: the balloon about GDScript code insight.
  "godot-editor-closed": godot(async (page) => {
    await openFile(page, "player.gd", 1);
    await page.waitForSelector(".balloon", { timeout: 10000 });
    await page.waitForTimeout(500);
  }, "&editor=closed"),
};
