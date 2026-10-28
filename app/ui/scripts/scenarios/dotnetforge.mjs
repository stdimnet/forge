/**
 * DotNetForge screenshots, on the `Bookstore` demo solution (`?project=dotnet`): the C#
 * editor with xUnit gutter icons, the Build tool window with the solution and a failed
 * build's problems (`&build=broken`), a dotnet test tree from the TRX report, the NuGet tool
 * window with a search and upgrades, the ".NET Project" run configuration with launch
 * profiles, Add Project from Template and Settings | .NET.
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

/** @param {import("playwright-core").Page} page */
async function runConfiguration(page, name) {
  await page.evaluate(async (n) => {
    const { launch } = await import("/src/features/run/runStore.ts");
    const { findConfiguration } = await import("/src/features/run/configStore.ts");
    await launch(findConfiguration(n));
  }, name);
}

const dotnet = (fn, query = "?project=dotnet") => Object.assign(fn, { query });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // CartTests.cs: ▶ on the test class and its xUnit facts and theories.
  "dotnet-editor": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "tests/Bookstore.Tests/CartTests.cs", 1);
    await page.waitForSelector(".run-gutter-icon:visible", { timeout: 15000 });
    await page.waitForTimeout(600);
  }),
  // The Build tool window: the solution tree and the problems of a failed build.
  "dotnet-build": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/Bookstore.Api/Program.cs", 12);
    await page.click('[data-tool-window="dotnet-build"]');
    await page.waitForSelector('[data-testid="dotnet-solution-tree"] .dn-row');
    await bottomHeight(page, 380);
    await page.click('[data-action="dotnet-build"]');
    await page.waitForSelector('[data-testid="dotnet-build-summary"]', { timeout: 20000 });
    await page.waitForSelector('[data-testid="dotnet-build-problem"]', { timeout: 5000 }).catch(() => {});
    await closeBalloons(page);
    await page.waitForTimeout(800);
  }, "?project=dotnet&build=broken"),
  // dotnet test: the TRX report read into the test tree, a failed FluentAssertions comparison.
  "dotnet-tests": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "tests/Bookstore.Tests/CartTests.cs", 18);
    await runConfiguration(page, "All tests");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => /failed/.test(document.querySelector(".run-tests-status")?.textContent ?? ""), null, { timeout: 20000 });
    await page.waitForTimeout(800);
    await bottomHeight(page, 360);
    await page.click('.test-row[data-test="Total_AddsLines"]').catch(() => {});
    await page.waitForTimeout(500);
  }),
  // The NuGet tool window: a search, a package's versions and the projects that use it.
  "dotnet-nuget": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/Bookstore.Api/Bookstore.Api.csproj", 1);
    await page.click('[data-tool-window="nuget"]');
    await page.waitForSelector('[data-testid="nuget-window"]');
    await bottomHeight(page, 380);
    await page.fill('[data-testid="nuget-search"]', "serilog");
    await page.waitForSelector('.nuget-item[data-package="Serilog.AspNetCore"]', { timeout: 10000 });
    await page.click('.nuget-item[data-package="Serilog.AspNetCore"]');
    await page.waitForSelector('[data-testid="nuget-version"]');
    await page.waitForTimeout(800);
  }),
  // NuGet upgrades of the solution's packages.
  "dotnet-nuget-updates": dotnet(async (page) => {
    await closeBalloons(page);
    await page.click('[data-tool-window="nuget"]');
    await page.waitForSelector('[data-testid="nuget-window"]');
    await bottomHeight(page, 340);
    await page.click('[data-testid="nuget-updates-tab"]');
    await page.waitForSelector(".nuget-item", { timeout: 10000 });
    await page.click(".nuget-item >> nth=0");
    await page.waitForTimeout(800);
  }),
  // Run | Edit Configurations: the ".NET Project" type with its launch profile.
  "dotnet-run-config": dotnet(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openRunConfigurations } = await import("/src/features/run/dialogState.ts");
      openRunConfigurations("Bookstore.Api: http");
    });
    await page.waitForTimeout(1200);
  }),
  // Add Project from Template: the templates of dotnet new list.
  "dotnet-add-project": dotnet(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("dotnet.addProject"));
    await page.waitForSelector('[data-testid="dotnet-templates"] .dn-add-item', { timeout: 10000 });
    await page.fill('[data-testid="dotnet-new-name"]', "Bookstore.Shipping");
    await page.click('.dn-add-item:has-text("Class Library")');
    await page.waitForTimeout(500);
  }),
  // Settings | Languages & Frameworks | .NET.
  "dotnet-settings": dotnet(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.dotnet"]');
    await page.waitForSelector('[data-testid="dotnet-settings"]');
    await page.waitForTimeout(800);
  }),
};
