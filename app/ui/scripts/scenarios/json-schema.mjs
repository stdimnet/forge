/**
 * JSON Schema: problems in composer.json, completion and hover from the bundled schemas, a
 * GitHub Actions workflow in YAML, the status bar widget with its schema popup, a bundled
 * schema opened by Go to Declaration and the JSON Schema Mappings settings page.
 */

/** @param {import("playwright-core").Page} page @param {string} path */
async function open(page, path) {
  await page.evaluate(async (path) => {
    const { openFile } = await import("/src/state/editor.ts");
    await openFile(path);
  }, path);
  await page.waitForTimeout(600);
}

/**
 * Replace the active editor's text, put the caret after `caret` (or at the end).
 * @param {import("playwright-core").Page} page @param {string} text @param {string} [caret]
 */
async function setText(page, text, caret) {
  await page.evaluate(
    async ([text, caret]) => {
      const { visibleView } = await import("/src/editor/views.ts");
      const { activeTab } = await import("/src/state/editor.ts");
      const view = await visibleView(activeTab().path);
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
      const at = caret === undefined ? text.length : text.indexOf(caret) + caret.length;
      view.dispatch({ selection: { anchor: at } });
      view.focus();
    },
    [text, caret],
  );
}

/** Hover the middle of the first occurrence of `text`. @param {import("playwright-core").Page} page @param {string} text */
async function hover(page, text) {
  const box = await page.evaluate(async (text) => {
    const { visibleView } = await import("/src/editor/views.ts");
    const { activeTab } = await import("/src/state/editor.ts");
    const view = await visibleView(activeTab().path);
    const at = view.state.doc.toString().indexOf(text) + Math.floor(text.length / 2);
    const c = view.coordsAtPos(at);
    return { x: c.left + 2, y: (c.top + c.bottom) / 2 };
  }, text);
  await page.mouse.move(box.x, box.y);
}

const COMPOSER = `{
    "name": "Acme/Shop",
    "type": "project",
    "minimum-stability": "nightly",
    "require": {
        "php": "^8.3",
        "laravel/framework": "^12.0"
    },
    "config": {
        "sort-packages": "yes"
    },
    "autoload": { "psr-4": { "App\\\\": "app/" } }
}
`;

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "json-schema-problems": async (page) => {
    await open(page, "composer.json");
    await setText(page, COMPOSER);
    await page.waitForTimeout(1200);
    await page.keyboard.press("Alt+Digit6");
    await page.waitForTimeout(800);
    await hover(page, '"nightly"');
    await page.waitForSelector(".cm-tooltip", { timeout: 5000 }).catch(() => undefined);
    await page.waitForTimeout(600);
  },

  "json-schema-completion": async (page) => {
    await open(page, "composer.json");
    await setText(page, `{\n    "name": "acme/shop",\n    "type": "project",\n    "\n}\n`, `"type": "project",\n    "`);
    await page.waitForTimeout(500);
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 });
    await page.keyboard.type("a");
    await page.waitForTimeout(900);
  },

  "json-schema-insert": async (page) => {
    await open(page, "composer.json");
    await setText(page, `{\n    "name": "acme/shop",\n    "\n    "type": "project"\n}\n`, `"acme/shop",\n    "`);
    await page.waitForTimeout(500);
    await page.keyboard.type("minimum");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 });
    await page.waitForTimeout(600);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 });
    await page.waitForTimeout(900);
  },

  "json-schema-hover": async (page) => {
    await open(page, "package.json");
    await page.waitForTimeout(800);
    await hover(page, '"scripts"');
    await page.waitForSelector(".cm-tooltip-hover", { timeout: 5000 });
    await page.waitForTimeout(500);
  },

  "json-schema-yaml": async (page) => {
    await open(page, ".github/workflows/tests.yml");
    await setText(
      page,
      "name: tests\non: [workflow_dispatch]\njobs:\n  phpunit:\n    runs-on: ubuntu-latest\n    timeout: 10\n    steps:\n      - uses: actions/checkout@v4\n      - run: composer install && php artisan test\n    \n",
      "php artisan test\n    ",
    );
    await page.waitForTimeout(900);
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 });
    await page.keyboard.type("co");
    await page.waitForTimeout(900);
  },

  "json-schema-status": async (page) => {
    await open(page, "docker-compose.yml");
    await page.waitForTimeout(1000);
    await page.click('[data-status="json-schema"]');
    await page.waitForTimeout(500);
  },

  "json-schema-declaration": async (page) => {
    await open(page, "composer.json");
    await page.waitForTimeout(500);
    await page.evaluate(async () => {
      const { visibleView } = await import("/src/editor/views.ts");
      const { activeTab } = await import("/src/state/editor.ts");
      const { gotoDeclaration } = await import("/src/features/lsp/goto.ts");
      const path = activeTab().path;
      const view = await visibleView(path);
      await gotoDeclaration(path, view, view.state.doc.toString().indexOf('"require"') + 2);
    });
    await page.waitForTimeout(1200);
  },

  "json-schema-settings": async (page) => {
    await page.evaluate(async () => {
      const { saveSetting } = await import("/src/state/settings.ts");
      await saveSetting(
        "jsonSchema.mappings",
        [
          { name: "Shop config", schema: "schemas/shop.schema.json", version: "7", files: "config/shop.json\nconfig/shops/\n*.shop.json" },
          { name: "No JSON schema", schema: "", version: "", files: "http/new-order.json" },
        ],
        "project",
      );
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("lang.json-schema");
    });
    await page.waitForSelector(".json-schema-page", { timeout: 8000 });
    await page.waitForTimeout(700);
  },

  "json-schema-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await open(page, "composer.json");
    await setText(page, COMPOSER);
    await page.waitForTimeout(1200);
    await hover(page, '"Acme/Shop"');
    await page.waitForSelector(".cm-tooltip", { timeout: 5000 }).catch(() => undefined);
    await page.waitForTimeout(600);
  },
};
