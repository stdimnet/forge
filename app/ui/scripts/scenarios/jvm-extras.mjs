/**
 * JavaForge's JVM extras: the Scala editor with the sbt tool window on the sbt demo
 * (`?project=scala`), a Groovy script run, the Device Manager and Logcat with filters on the
 * Android demo (`?project=android`), and the SDK hint without an Android SDK.
 */

const scala = (fn) => Object.assign(fn, { query: "?project=scala" });
const android = (fn) => Object.assign(fn, { query: "?project=android" });

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1, 1);
  }, path);
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
  await page.waitForTimeout(400);
}

/** Expand the project tree down to `path`'s folder (a click toggles a folder). */
async function reveal(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await page.locator(`.tree-row[data-path="${dir}"]`).first().click();
    await page.waitForTimeout(120);
  }
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

async function logcat(page, light) {
  if (light) await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await closeBalloons(page);
  await page.click('[data-tool-window="logcat"]');
  await page.waitForSelector(".logcat-row", { timeout: 15000 });
  await bottomHeight(page, 380);
  await page.selectOption('[data-testid="logcat-level"]', "W");
  await page.waitForTimeout(600);
  await page.waitForSelector(".logcat-link");
  await closeBalloons(page);
  await page.waitForTimeout(300);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "scala-editor-sbt": scala(async (page) => {
    await closeBalloons(page);
    await reveal(page, "core/src/test/scala/com/acme/shop/CartSpec.scala");
    await page.waitForSelector('.cm-line-marker[title="Run \'report\'"]', { timeout: 10000 });
    await page.click('[data-tool-window="sbt"]');
    await page.waitForSelector('[data-testid="sbt-tool-window"]');
    await page.click('[data-testid="sbt-reload"]');
    await page.waitForSelector('[data-testid="sbt-import-status"]');
    await page.waitForSelector('[data-testid="sbt-import-status"]', { state: "detached", timeout: 20000 });
    await page.dblclick('[data-sbt-project="core"]');
    await page.dblclick('[data-sbt-deps="core"]');
    await page.waitForTimeout(400);
    await closeBalloons(page);
  }),
  "scala-test-tree": scala(async (page) => {
    await closeBalloons(page);
    await reveal(page, "core/src/test/scala/com/acme/shop/CartSpec.scala");
    await openFile(page, "core/src/test/scala/com/acme/shop/CartSpec.scala");
    await page.waitForSelector(".run-gutter-icon:visible", { timeout: 10000 });
    await page.locator(".run-gutter-icon:visible").first().click();
    await page.waitForSelector(".run-popup-menu");
    await page.click(`.run-popup-menu .run-menu-item:has-text("Run 'CartSpec'")`);
    await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("failed"), null, { timeout: 20000 });
    await bottomHeight(page, 330);
    await page.waitForTimeout(500);
  }),
  "gradle-source-sets": Object.assign(
    async (page) => {
      await closeBalloons(page);
      await page.waitForSelector('[data-tool-window="gradle"]', { timeout: 10000 });
      await page.click('[data-tool-window="gradle"]');
      await page.waitForSelector('.java-bt-row[data-source-sets]', { timeout: 15000 });
      for (const n of ["Tasks", "Dependencies"]) await page.locator(".java-bt-row", { hasText: n }).first().locator(".java-bt-twisty").click();
      await page.click(".java-bt-row[data-source-sets] .java-bt-twisty");
      for (const set of ["main", "test"]) await page.click(`.java-bt-row[data-source-set="${set}"] .java-bt-twisty`);
      await page.waitForTimeout(300);
    },
    { query: "?project=java" },
  ),
  "groovy-script-run": android(async (page) => {
    await closeBalloons(page);
    await reveal(page, "scripts/release.groovy");
    await openFile(page, "scripts/release.groovy");
    const marker = page.locator(`.cm-line-marker[title="Run 'release.groovy'"]`).first();
    await marker.waitFor({ timeout: 10000 });
    await marker.click();
    await page.waitForSelector(".run-popup-menu");
    await page.click(`.run-popup-menu .run-menu-item:has-text("Run 'release.groovy'")`);
    await page.waitForFunction(() => document.body.textContent?.includes("Process finished"), null, { timeout: 15000 });
    await bottomHeight(page, 260);
    await closeBalloons(page);
    await page.waitForTimeout(400);
  }),
  "android-devices": android(async (page) => {
    await closeBalloons(page);
    await page.click('[data-tool-window="android-devices"]');
    await page.waitForSelector(".android-device-row");
    await page.waitForTimeout(400);
  }),
  "android-logcat": android(async (page) => logcat(page, false)),
  "android-logcat-light": android(async (page) => logcat(page, true)),
  "android-sdk-missing": Object.assign(
    async (page) => {
      await page.waitForSelector(".balloon", { timeout: 15000 });
      await page.click('[data-tool-window="android-devices"]');
      await page.waitForSelector('[data-testid="android-sdk-hint"]');
      await page.waitForTimeout(400);
    },
    { query: "?project=android&android-sdk=missing" },
  ),
};
