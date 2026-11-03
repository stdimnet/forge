/**
 * Rails screenshots, on the `blog` demo application (`?project=rails`): the Rails routes in
 * the Endpoints tool window, a controller with its gutter icons and folded translations, a
 * view with Stimulus and attribute completion, Run Rails Generator, Go to Related, data
 * sources from database.yml, the Rails server run and Settings | Rails.
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

const rails = (fn) => Object.assign(fn, { query: "?project=rails" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  // Endpoints: the routes of config/routes.rb as bin/rails routes prints them.
  "rails-endpoints": rails(async (page) => {
    await closeBalloons(page);
    await openFile(page, "config/routes.rb", 6);
    await page.evaluate(() => window.__forgeRunCommand("rails.showRoutes"));
    await page.waitForSelector("[data-route]");
    await bottomHeight(page, 420);
    await page.click('[data-route="PATCH,PUT /articles/:id"]').catch(() => {});
    await page.waitForTimeout(600);
  }),
  // A controller: views and routes in the gutter, t(".created") folded to its text.
  "rails-controller": rails(async (page) => {
    await openFile(page, "app/controllers/articles_controller.rb", 4);
    await page.waitForSelector(".marker-rails-view");
    await page.waitForSelector(".cm-rails-translation-fold");
    await page.waitForTimeout(800);
    const marker = page.locator(".marker-rails-route").first();
    await marker.hover();
    await page.waitForTimeout(900);
  }),
  // A view: Stimulus icons, a missing translation, completion of model attributes.
  "rails-view": rails(async (page) => {
    await closeBalloons(page);
    await openFile(page, "app/views/articles/_form.html.erb", 12);
    await page.waitForSelector(".marker-rails-stimulus");
    const line = page.locator(".cm-line", { hasText: "form.textarea :body" }).first();
    await line.click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("<%= form.text_field :", { delay: 40 });
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(800);
  }),
  // Run Rails Generator (Ctrl+Alt+G): generators from bin/rails generate with their usage.
  "rails-generator": rails(async (page) => {
    await closeBalloons(page);
    await openFile(page, "app/controllers/articles_controller.rb", 4);
    await page.keyboard.press("Control+Alt+KeyG");
    await page.waitForSelector(".rails-gen-popup");
    await page.waitForSelector("[data-generator]");
    await page.keyboard.type("m", { delay: 15 });
    await page.waitForTimeout(600);
  }),
  // Go to Related (Ctrl+Alt+Home) on a model: controller, views, helper, tests, migrations, routes.
  "rails-related": rails(async (page) => {
    await closeBalloons(page);
    await openFile(page, "app/models/article.rb", 1);
    await page.waitForSelector(".marker-rails-table");
    await page.evaluate(() => window.__forgeRunCommand("GotoRelated"));
    await page.waitForSelector(".decl-chooser, .declaration-chooser, [role=listbox]", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(700);
  }),
  // Data sources from config/database.yml in the Database tool window.
  "rails-database": rails(async (page) => {
    await page.waitForSelector(".balloon", { timeout: 8000 }).catch(() => {});
    await openFile(page, "config/database.yml", 9);
    await page.waitForTimeout(400);
    await page.evaluate(() => window.__forgeRunCommand("rails.addDataSources"));
    await page.waitForSelector('[data-tool-window-panel="database"]', { timeout: 8000 }).catch(() => {});
    await page.waitForTimeout(1200);
  }),
  // The "Rails" run configuration: bin/rails server in the Run tool window.
  "rails-server": rails(async (page) => {
    await closeBalloons(page);
    await openFile(page, "app/controllers/articles_controller.rb", 4);
    await page.evaluate(async () => {
      const { launch } = await import("/src/features/run/runStore.ts");
      const { findConfiguration } = await import("/src/features/run/configStore.ts");
      await launch(findConfiguration("blog: development"));
    });
    await page.waitForTimeout(2400);
    await bottomHeight(page, 340);
    await page.waitForTimeout(400);
  }),
  // Settings | Languages & Frameworks | Ruby | Rails.
  "rails-settings": rails(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.click('.set-tree-row[data-node="lang.rails"]').catch(async () => {
      await page.click('.set-tree-row[data-node="lang.ruby"]');
      await page.click('.set-tree-row[data-node="lang.rails"]');
    });
    await page.waitForTimeout(800);
  }),
};
