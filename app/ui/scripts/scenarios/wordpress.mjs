/** WordPress and Drupal screenshots, on the demo plugin (`?project=wordpress`) and module (`?project=drupal`). */

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const row = page.locator(`.tree-row[data-path="${dir}"]`);
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await row.click();
  }
  await page.locator(`.tree-row[data-path="${path}"]`).click();
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** @param {import("playwright-core").Page} page */
async function lineLocator(page, text) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first();
  for (let i = 0; i < 20 && !(await line.count()); i++) {
    await page.evaluate(() => {
      const s = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
      if (s) s.scrollTop += 300;
    });
    await page.waitForTimeout(80);
  }
  await line.scrollIntoViewIfNeeded();
  return line;
}

/** @param {import("playwright-core").Page} page */
async function caretAtLineEnd(page, text) {
  const line = await lineLocator(page, text);
  await line.click();
  await page.keyboard.press("End");
}

/** Put the caret right after `needle` on the line containing it. */
async function caretAfter(page, lineText, needle) {
  const line = await lineLocator(page, lineText);
  await line.click();
  await page.keyboard.press("Home");
  const text = await line.innerText();
  const at = text.indexOf(needle) + needle.length;
  const lead = text.length - text.trimStart().length;
  for (let i = lead; i < at; i++) await page.keyboard.press("ArrowRight");
}

/** @param {import("playwright-core").Page} page */
async function ready(page, label = "WordPress") {
  await page.waitForSelector(".status-item", { hasText: label }).catch(() => {});
  await page.waitForTimeout(600);
  // The stubs balloon is not part of these screenshots.
  await page.evaluate(() => document.querySelectorAll(".balloon-close, .balloon .icon-btn").forEach((b) => b.click?.()));
}

/** @param {import("playwright-core").Page} page */
async function showTool(page, id) {
  if (!(await page.locator(`[data-tool-window-panel="${id}"]`).isVisible().catch(() => false))) {
    await page.evaluate((tw) => window.__forgeRunCommand(tw === "wordpress" ? "WordPress.ShowHooks" : tw), id);
  }
  await page.waitForSelector(`[data-tool-window-panel="${id}"]`);
  const splitter = page.locator(".splitter-h").first();
  if (await splitter.isVisible().catch(() => false)) {
    const box = await splitter.boundingBox();
    if (box) {
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width / 2, box.y - 130, { steps: 5 });
      await page.mouse.up();
    }
  }
  await page.waitForTimeout(200);
}

/** Undo horizontal scrolling caused by moving the caret. */
async function scrollHome(page) {
  await page.evaluate(() => {
    const s = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
    if (s) s.scrollLeft = 0;
  });
}

const wp = (fn) => Object.assign(fn, { query: "?project=wordpress" });
const drupal = (fn) => Object.assign(fn, { query: "?project=drupal" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "wordpress-hooks": wp(async (page) => {
    await ready(page);
    await openFile(page, "includes/class-acme-events.php");
    await showTool(page, "wordpress");
    await page.waitForSelector(".wp-table tbody tr");
    await page.locator('.wp-table tr[data-hook="acme_events_registered"]').click();
    await page.waitForTimeout(200);
  }),
  "wordpress-hooks-light": wp(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await ready(page);
    await openFile(page, "includes/class-acme-events.php");
    await showTool(page, "wordpress");
    await page.waitForSelector(".wp-table tbody tr");
    await page.locator(".wp-kind", { hasText: "Filters" }).click();
    await page.waitForTimeout(200);
  }),
  "wordpress-hook-completion": wp(async (page) => {
    await ready(page);
    await openFile(page, "includes/functions.php");
    await caretAtLineEnd(page, "add_filter( 'acme_events_label_2026-12-24'");
    await page.keyboard.press("Enter");
    await page.keyboard.type("add_action( 'acme_");
    await page.evaluate(() => {
      const s = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
      if (s) s.scrollLeft = 0;
    });
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(400);
  }),
  "wordpress-gutter": wp(async (page) => {
    await ready(page);
    await openFile(page, "includes/class-acme-events.php");
    await page.waitForSelector(".editor-slot:not(.hidden) .cm-line-marker.marker-wp-registration");
    const marker = page.locator(".editor-slot:not(.hidden) .cm-line-marker.marker-wp-invocation").nth(1);
    await marker.hover();
    await page.waitForTimeout(900);
  }),
  "wordpress-goto-hook": wp(async (page) => {
    await ready(page);
    await openFile(page, "includes/class-acme-events.php");
    await caretAfter(page, "$label  = apply_filters(", "acme_events_lab");
    await page.keyboard.press("Control+KeyB");
    await page.waitForTimeout(800);
  }),
  "wordpress-usages": wp(async (page) => {
    await ready(page);
    await openFile(page, "includes/class-acme-events.php");
    await caretAfter(page, "do_action( 'acme_events_registered'", "acme_events_reg");
    await page.keyboard.press("Alt+F7");
    await page.waitForSelector(".find-window, [data-tool-window-panel=\"find\"]");
    await scrollHome(page);
    await page.waitForTimeout(600);
  }),
  "wordpress-yoda": wp(async (page) => {
    await ready(page);
    await openFile(page, "includes/functions.php");
    await caretAfter(page, "if ( $post->post_status == 'auto-draft' )", "$post->post_");
    await page.waitForTimeout(900);
    await scrollHome(page);
    await page.keyboard.press("Alt+Enter");
    await page.waitForSelector(".intention-popup, .intentions-popup, .context-actions-popup", { timeout: 4000 }).catch(() => {});
    await page.waitForTimeout(400);
  }),
  "wordpress-wp-cli": wp(async (page) => {
    await ready(page);
    await page.evaluate(() => window.__forgeRunCommand("WordPress.RunWpCli"));
    await page.waitForTimeout(500);
    await page.keyboard.type("plugin ");
    await page.waitForTimeout(600);
  }),
  "drupal-hook-completion": drupal(async (page) => {
    await ready(page, "Drupal");
    await openFile(page, "acme_tools.module");
    const last = await lineLocator(page, "'variables' => ['items' => []],");
    await last.click();
    await page.keyboard.press("Control+End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("function acme_tools_form");
    await page.waitForSelector(".cm-tooltip-autocomplete li");
    await page.waitForTimeout(400);
  }),
};
