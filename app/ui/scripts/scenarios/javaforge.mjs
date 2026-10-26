/**
 * JavaForge screenshots on the Gradle demo service (`?project=java`): the Java editor with
 * Eclipse JDT LS completion, Settings | JDKs, the Run gutter on `main`, a JUnit 5 test tree
 * from Gradle's reports, a java-debug session, the Gradle tool window after a sync (tasks,
 * resolved dependencies), Execute Gradle Task, the Maven tool window with profiles
 * (`?build=maven`), Kotlin DSL and coordinate completion, the outdated-version quick fix and
 * Settings | Build Tools | Gradle with the wrapper.
 */

const java = (fn) => Object.assign(fn, { query: "?project=java" });
const PKG = "src/main/java/com/acme/inventory";
const TEST_PKG = "src/test/java/com/acme/inventory";

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

/** Put the caret at the end of the line containing `text`. */
async function caretAfter(page, text) {
  const line = page.locator(".editor-slot:not(.hidden) .cm-line", { hasText: text }).first();
  await line.click();
  await page.keyboard.press("End");
}

/** Click the test gutter icon on the line containing `text` and pick `item`. */
async function testGutter(page, text, item) {
  const line = page.locator(".cm-line:visible", { hasText: text }).first();
  const box = await line.boundingBox();
  const icons = page.locator(".cm-run-gutter:visible .cm-gutterElement");
  for (let i = 0; i < (await icons.count()); i++) {
    const b = await icons.nth(i).boundingBox();
    if (b && box && Math.abs(b.y - box.y) < 4 && (await icons.nth(i).locator(".run-gutter-icon").count())) {
      await icons.nth(i).locator(".run-gutter-icon").click();
      break;
    }
  }
  await page.waitForSelector(".run-popup-menu");
  if (item) await page.click(`.run-popup-menu .run-menu-item:has-text("${item}")`);
}

/** Click the line number `line` in the gutter (toggles a breakpoint). */
async function breakpoint(page, line) {
  await page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: new RegExp(`^${line}$`) }).first().click();
  await page.waitForSelector(".cm-bp-dot");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "java-editor-completion": java(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${PKG}/App.java`);
    await openFile(page, `${PKG}/App.java`);
    await page.waitForSelector('.cm-line-marker[title="Run \'App.main()\'"]', { timeout: 10000 });
    await caretAfter(page, 'inventory.remove("SKU-2002", 5);');
    await page.keyboard.press("Enter");
    await page.keyboard.type("inventory.lo");
    await page.keyboard.press("Control+Space");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(500);
  }),
  "java-run-gutter": java(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${PKG}/App.java`);
    await openFile(page, `${PKG}/App.java`);
    const marker = page.locator('.cm-line-marker[title="Run \'App.main()\'"]').first();
    await marker.waitFor({ timeout: 10000 });
    await marker.click();
    await page.waitForSelector(".run-popup-menu");
    await page.waitForTimeout(300);
  }),
  "java-junit-tree": java(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${TEST_PKG}/InventoryTest.java`);
    await openFile(page, `${TEST_PKG}/InventoryTest.java`);
    await page.waitForSelector(".run-gutter-icon:visible", { timeout: 10000 });
    await testGutter(page, "class InventoryTest", "Run 'InventoryTest'");
    await page.waitForSelector(".run-tests-status");
    await page.waitForFunction(() => document.querySelector(".run-tests-status")?.textContent?.includes("failed"), null, { timeout: 20000 });
    await bottomHeight(page, 330);
    await page.click('.test-row[data-test="removesStock()"]').catch(() => {});
    await page.waitForTimeout(500);
  }),
  "java-debug-session": java(async (page) => {
    await closeBalloons(page);
    await reveal(page, `${PKG}/Inventory.java`);
    await openFile(page, `${PKG}/Inventory.java`);
    await breakpoint(page, 22);
    await openFile(page, `${PKG}/App.java`);
    const marker = page.locator('.cm-line-marker[title="Run \'App.main()\'"]').first();
    await marker.waitFor({ timeout: 10000 });
    await marker.click();
    await page.click('.run-popup-menu .run-menu-item:has-text("Debug \'App.main()\'")');
    await page.waitForSelector(".cm-exec-line", { timeout: 20000 });
    await page.waitForSelector('.dbg-row[data-var="current"]', { timeout: 10000 });
    await bottomHeight(page, 340);
    await page.click('.dbg-row[data-var="current"] .tree-chevron').catch(() => {});
    await page.waitForTimeout(600);
  }),
  "java-jdks-settings": java(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("build.jdks");
    });
    await page.waitForSelector(".java-sdk-row");
    await page.click('.java-sdk-row[data-jdk="temurin-21"]');
    await page.waitForTimeout(300);
  }),
  "java-jdks-settings-light": java(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("build.jdks");
    });
    await page.waitForSelector(".java-sdk-row");
    await page.click('.java-sdk-row[data-jdk="corretto-17"]');
    await page.waitForTimeout(300);
  }),
  "java-jdk-download": java(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("java.downloadJdk"));
    await page.waitForSelector("[data-java-download]");
    await page.waitForTimeout(400);
  }),
  "java-settings-jdtls": java(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("lang.java");
    });
    await page.waitForSelector("[data-java-status]");
    await page.waitForTimeout(400);
  }),
  "java-gradle-tool-window": java(async (page) => {
    await closeBalloons(page);
    await page.waitForSelector('[data-tool-window="gradle"]', { timeout: 10000 });
    await page.click('[data-tool-window="gradle"]');
    await page.waitForSelector('[data-testid="gradle-sync"]');
    await page.click('[data-testid="gradle-sync"]');
    await page.waitForSelector('.java-bt-row[data-configuration="testRuntimeClasspath"]', { timeout: 15000 });
    await closeBalloons(page);
    await page.click('.java-bt-row[data-build-group="build"] .java-bt-twisty').catch(() => {});
    for (const cfg of ["compileClasspath", "testRuntimeClasspath"]) await page.click(`.java-bt-row[data-configuration="${cfg}"] .java-bt-twisty`);
    await page.click('.java-bt-row[data-dependency="com.google.guava:guava:33.3.1-jre"] .java-bt-twisty');
    await page.click('.java-bt-row[data-goal="test"]').catch(() => {});
    await page.waitForTimeout(300);
  }),
  "java-gradle-execute-task": java(async (page) => {
    await closeBalloons(page);
    await page.waitForSelector('[data-tool-window="gradle"]', { timeout: 10000 });
    await page.click('[data-tool-window="gradle"]');
    await page.click('[data-testid="gradle-execute"]');
    await page.waitForSelector('[data-testid="java-execute-input"]');
    await page.keyboard.type("clean jaco");
    await page.waitForTimeout(300);
  }),
  "java-maven-tool-window": Object.assign(
    async (page) => {
      await closeBalloons(page);
      await page.waitForSelector('[data-tool-window="maven"]', { timeout: 10000 });
      await page.click('[data-tool-window="maven"]');
      await page.waitForSelector('.java-bt-row[data-profile="dev"]');
      await page.click('.java-bt-row[data-profile="integration-tests"] .java-bt-check');
      await page.click('[data-testid="maven-skip-tests"]');
      await page.click('[data-testid="maven-sync"]');
      await page.waitForSelector('.java-bt-row[data-dependency="org.junit.jupiter:junit-jupiter:5.10.2"]', { timeout: 15000 });
      await closeBalloons(page);
      await page.click('.java-bt-row:has-text("Lifecycle") .java-bt-twisty');
      await page.click('.java-bt-row[data-dependency="com.google.guava:guava:32.1.3-jre"] .java-bt-twisty');
      await page.click('.java-bt-row[data-dependency="org.junit.jupiter:junit-jupiter:5.10.2"] .java-bt-twisty');
      await page.waitForTimeout(300);
    },
    { query: "?project=java&build=maven" },
  ),
  "java-gradle-kts-completion": java(async (page) => {
    await closeBalloons(page);
    await openFile(page, "build.gradle.kts");
    await caretAfter(page, 'implementation("com.google.guava:guava:33.3.1-jre")');
    await page.keyboard.press("Enter");
    await page.keyboard.type('testImplementation("org.assertj:');
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(600);
  }),
  "java-gradle-kts-dsl-completion": java(async (page) => {
    await closeBalloons(page);
    await openFile(page, "build.gradle.kts");
    await caretAfter(page, 'implementation("com.google.guava:guava:33.3.1-jre")');
    await page.keyboard.press("Enter");
    await page.keyboard.type("test");
    await page.waitForSelector(".cm-tooltip-autocomplete", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(500);
  }),
  "java-pom-outdated": Object.assign(
    async (page) => {
      await closeBalloons(page);
      await openFile(page, "pom.xml");
      await page.waitForSelector(".marker-composer-outdated", { timeout: 10000 }).catch(() => {});
      await caretAfter(page, "<guava.version>");
      await page.keyboard.press("Alt+Enter");
      await page.waitForTimeout(600);
    },
    { query: "?project=java&build=maven" },
  ),
  "java-gradle-settings": java(async (page) => {
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("build.tools.gradle");
    });
    await page.waitForSelector('[data-testid="gradle-wrapper"]');
    await page.waitForTimeout(400);
  }),
  "java-gradle-settings-light": java(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await closeBalloons(page);
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("build.tools.gradle");
    });
    await page.waitForSelector('[data-testid="gradle-wrapper"]');
    await page.click('[data-testid="gradle-update-wrapper"]');
    await page.waitForSelector(".java-hint", { timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(300);
  }),
  "java-jdtls-install": Object.assign(
    async (page) => {
      await page.waitForSelector(".balloon", { timeout: 15000 });
      await page.waitForTimeout(400);
    },
    { query: "?project=java&jdtls=missing" },
  ),
};
