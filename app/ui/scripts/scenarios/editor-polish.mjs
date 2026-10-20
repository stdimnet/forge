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

const reactChooser = async (page) => {
  await openFromTree(page, "src/components/ui/Button.tsx");
  // On a component's own name: its usages, in the same popup.
  await caretAt(page, "const Button", 8);
  await page.waitForTimeout(800);
  await page.keyboard.press("Control+KeyB");
  await page.waitForSelector(".decl-chooser-row");
};
reactChooser.query = "?project=react";

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "editor-polish-choose-declaration-lua": luaChooser,
  "editor-polish-choose-declaration-react": reactChooser,
};
