/**
 * DotNetForge for web and cloud projects, on the `Bookstore` demo solution
 * (`?project=dotnet`): ASP.NET Core endpoints, Razor / Blazor in the editor with parameter
 * completion, the EF Core tool window and its dialog, the Aspire AppHost in Services, Add
 * Docker Support, Hot Reload in the status bar, user secrets in an editor tab and the Azure
 * DevOps accounts page.
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

const command = (page, id) => page.evaluate((c) => window.__forgeRunCommand(c), id);
const dotnet = (fn, query = "?project=dotnet") => Object.assign(fn, { query });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // The Endpoints tool window: minimal API routes with groups, controller actions, Blazor pages.
  "dotnet-endpoints": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/Bookstore.Api/Program.cs", 16);
    await command(page, "dotnet.showEndpoints");
    await page.waitForSelector('[data-testid="aspnet-endpoints"] tr[data-endpoint="GET /api/authors/{id:int}"]', { timeout: 20000 });
    await bottomHeight(page, 400);
    await page.click('tr[data-endpoint="GET /api/authors/{id:int}"]');
    await page.waitForTimeout(600);
  }),
  // Generate HTTP Request: the .http request of an endpoint in the HTTP Client.
  "dotnet-http-request": dotnet(async (page) => {
    await closeBalloons(page);
    await command(page, "dotnet.showEndpoints");
    await page.waitForSelector('tr[data-endpoint="POST /api/Orders"]', { timeout: 20000 });
    await bottomHeight(page, 300);
    await page.click('tr[data-endpoint="POST /api/Orders"] [data-testid="generate-http-request"]');
    await page.waitForSelector('.editor-tab.active[title^="scratch://"]', { timeout: 10000 });
    await page.waitForTimeout(800);
  }),
  // A Blazor page: C# inside Razor, and the parameters of <Rating> in completion.
  "dotnet-razor": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/Bookstore.Api/Components/Pages/Shelf.razor", 18);
    await page.waitForSelector(".cm-razor-transition", { timeout: 10000 });
    // Complete a parameter inside the <Rating …> tag.
    await page.evaluate(async () => {
      const { viewFor } = await import("/src/editor/views.ts");
      const view = viewFor("src/Bookstore.Api/Components/Pages/Shelf.razor");
      const line = view.state.doc.line(19);
      const at = line.from + line.text.indexOf("/>");
      view.dispatch({ selection: { anchor: at } });
      view.focus();
    });
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(700);
  }),
  // The EF Core tool window after Refresh: applied and pending migrations.
  "dotnet-efcore": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/Bookstore.Api/Data/BookstoreContext.cs", 7);
    await command(page, "dotnet.ef.listMigrations");
    // dotnet ef runs in the Run tool window; then back to EF Core.
    await page.waitForTimeout(6000);
    await closeBalloons(page);
    await page.click('[data-tool-window="efcore"]');
    await page.waitForSelector('[data-testid="efcore-window"] tr[data-migration="AddReviews"] .dn-ef-status.pending', { timeout: 25000 });
    await bottomHeight(page, 340);
    await page.waitForTimeout(600);
  }),
  // EF Core: Add Migration.
  "dotnet-ef-add-migration": dotnet(async (page) => {
    await closeBalloons(page);
    await command(page, "dotnet.ef.addMigration");
    await page.waitForSelector('[data-testid="ef-dialog-addMigration"]', { timeout: 15000 });
    await page.fill('[data-testid="ef-migration-name"]', "AddWishlist");
    await page.waitForTimeout(500);
  }),
  // .NET Aspire: the AppHost and its resources in Services, with the dashboard link.
  "dotnet-aspire": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/Bookstore.AppHost/Program.cs", 10);
    await runConfiguration(page, "Bookstore.AppHost");
    await page.waitForTimeout(3500);
    await closeBalloons(page);
    await page.click('[data-tool-window="services"]');
    const node = '.svc-row[data-node-id="aspire/src/Bookstore.AppHost/Bookstore.AppHost.csproj"]';
    await page.waitForSelector(node, { timeout: 15000 });
    await page.click(node);
    await page.waitForSelector('.svc-section[data-section="Resources"]', { timeout: 15000 });
    await bottomHeight(page, 420);
    await page.waitForTimeout(800);
  }),
  // Add Docker Support: the generated multi-stage Dockerfile.
  "dotnet-docker": dotnet(async (page) => {
    await closeBalloons(page);
    await command(page, "dotnet.addDockerSupport");
    await page.waitForSelector('[data-testid="dockerfile-preview"]', { timeout: 15000 });
    await page.waitForTimeout(500);
  }),
  // Hot Reload: a dotnet watch run with its status in the status bar.
  "dotnet-hot-reload": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/Bookstore.Api/Program.cs", 12);
    await runConfiguration(page, "Bookstore.Api (watch)");
    await page.waitForSelector('[data-status="hot-reload"].ok', { timeout: 25000 });
    await closeBalloons(page);
    await bottomHeight(page, 300);
    await page.hover('[data-status="hot-reload"]');
    await page.waitForTimeout(800);
  }),
  // Manage User Secrets: secrets.json of the API in an editor tab (outside the project).
  "dotnet-user-secrets": dotnet(async (page) => {
    await closeBalloons(page);
    await openFile(page, "src/Bookstore.Api/Program.cs", 1);
    await command(page, "dotnet.manageUserSecrets");
    await page.waitForSelector('.editor-tab.active[title^="usersecrets://"]', { timeout: 15000 });
    await page.waitForTimeout(800);
  }),
  // Settings | Version Control | Azure DevOps.
  "azure-devops-accounts": dotnet(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="vcs.hosts"]').catch(() => {});
    await page.click('.set-tree-row[data-node="vcs.azure"]');
    await page.waitForSelector('[data-hosts-page="azure"]');
    await page.click('[data-action="add-account"]');
    await page.waitForSelector("[data-login-form]");
    await page.waitForTimeout(500);
  }),
};
