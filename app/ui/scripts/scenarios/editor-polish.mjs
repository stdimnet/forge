/**
 * Editor polish: the shared "Choose Declaration" popup, the toolbar of `.http` editors with
 * the environment selector and response handler output, Find in Files and the status bar of
 * the demo projects.
 */

/** Open a project file through the Project view (single clicks expand folders). */
async function openFromTree(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await page.locator(`.tree-row[data-path="${dir}"]`).click();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** Put the caret `delta` characters into the first occurrence of `needle`. */
async function caretAt(page, needle, delta) {
  await page.evaluate(
    ([needle, delta]) => {
      const view = document.querySelector(".editor-slot:not(.hidden) .cm-content")?.cmTile?.view;
      const at = view.state.doc.toString().indexOf(needle) + delta;
      view.dispatch({ selection: { anchor: at }, scrollIntoView: true });
      view.focus();
    },
    [needle, delta],
  );
}

/** Close balloons so they do not cover the bottom tool window. */
async function dismissBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
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

/** Run output contains `text`. */
async function waitForOutput(page, text) {
  await page.waitForFunction((t) => [...document.querySelectorAll(".run-line")].some((l) => l.textContent?.includes(t)), text, {
    timeout: 10000,
  });
}

const luaChooser = async (page) => {
  await openFromTree(page, "src/ui/hud.lua");
  // The global `DEBUG` is assigned in conf.lua and main.lua.
  await caretAt(page, "if DEBUG", 4);
  await page.waitForTimeout(800);
  await page.keyboard.press("Control+KeyB");
  await page.waitForSelector(".decl-chooser-row");
};
luaChooser.query = "?project=lua";

const vueStatus = async (page) => {
  // Branch, failed CI and no Docker widget: the Vue demo's own status bar.
  await page.waitForSelector('[data-status="pipeline"]');
  await page.waitForTimeout(800);
};
vueStatus.query = "?project=vue";

const reactChooser = async (page) => {
  await openFromTree(page, "src/components/ui/Button.tsx");
  // On a component's own name: its usages, in the same popup.
  await caretAt(page, "const Button", 8);
  await page.waitForTimeout(800);
  await page.keyboard.press("Control+KeyB");
  await page.waitForSelector(".decl-chooser-row");
};
reactChooser.query = "?project=react";

/** Expand "Scratches and Consoles" > "Scratches" (collapsing `app`) and open a scratch. */
async function openScratch(page, name) {
  const app = page.locator('.tree-row[data-path="app"]');
  if ((await app.getAttribute("data-expanded")) !== null) await app.locator(".tree-chevron").click();
  await page.locator('.tree-row[data-path="scratches-and-consoles://"] .tree-chevron').click();
  await page.locator('.tree-row[data-path="scratches-and-consoles://scratches"] .tree-chevron').click();
  await page.locator(`.tree-row[data-path="scratch://${name}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="scratch://${name}"]`);
  await page.waitForSelector(".http-toolbar");
  await page.waitForSelector(".cm-line-marker.marker-run");
  await page.waitForTimeout(200);
}

/** Click the gutter ▶ of the request on 0-based `line`. */
async function runRequestAt(page, line) {
  await page.evaluate((line) => {
    const view = document.querySelector(".editor-slot:not(.hidden) .cm-content")?.cmTile?.view;
    const pos = view.state.doc.line(line + 1).from;
    view.dispatch({ selection: { anchor: pos } });
  }, line);
  await page.evaluate(() => window.__forgeRunCommand("RunClass"));
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "editor-polish-choose-declaration-lua": luaChooser,
  "editor-polish-choose-declaration-react": reactChooser,
  "editor-polish-status-vue": vueStatus,
  "editor-polish-http-environments": async (page) => {
    await dismissBalloons(page);
    await openScratch(page, "rest-api.http");
    await page.click('[data-testid="http-env"]');
    await page.waitForSelector(".http-tb-item");
  },
  "editor-polish-http-handler": async (page) => {
    await dismissBalloons(page);
    await openScratch(page, "rest-api.http");
    // Choose "staging" in the toolbar, log in (the handler stores the token), create an order.
    await page.click('[data-testid="http-env"]');
    await page.click('.http-tb-item:has-text("staging")');
    await runRequestAt(page, 1);
    await waitForOutput(page, "Tests:");
    await runRequestAt(page, 28);
    await waitForOutput(page, "Created order");
    await waitForOutput(page, "POST http://staging.acme-shop.test/api/orders");
    await bottomHeight(page, 420);
    await page.waitForTimeout(300);
  },
  "editor-polish-http-body-file": async (page) => {
    await dismissBalloons(page);
    // A project .http file: `< ./new-order.json` is read next to it, {{$uuid}} is generated.
    await openFromTree(page, "http/orders.http");
    await page.waitForSelector(".http-toolbar");
    await page.waitForSelector(".cm-line-marker.marker-run");
    await runRequestAt(page, 1);
    await waitForOutput(page, "Tests:");
    await bottomHeight(page, 380);
    await page.waitForTimeout(300);
  },
  "editor-polish-find-in-files": async (page) => {
    await dismissBalloons(page);
    await page.keyboard.press("Control+Shift+KeyF");
    await page.waitForSelector(".find-input");
    await page.keyboard.type("orders()", { delay: 15 });
    await page.click('.find-toggle[title="Words"]');
    await page.check(".find-mask input[type=checkbox]");
    await page.waitForSelector(".find-row");
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(500);
  },
};
