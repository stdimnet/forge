/**
 * Settings that now apply: status bar popups (line separator, encoding, indent), the
 * Editor, Inlay Hints and Git settings pages, inlay hints and Code Vision in the editor,
 * commit message inspections, the restored tool window layout and the AI chat history.
 */

/** Bottom-right corner of the window (status bar popups). */
const statusClip = { x: 840, y: 560, width: 600, height: 340 };

/** @param {import("playwright-core").Page} page */
async function openSettingsPage(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.waitForSelector(".set-row, .plugin-item");
  const row = page.locator(`.set-tree-row[data-node="${id}"]`);
  if (!(await row.count())) {
    // Child pages are folded under their parent.
    const parent = id.split(".").slice(0, -1).join(".");
    await page.click(`.set-tree-row[data-node="${parent}"] .set-tree-chevron`);
  }
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForTimeout(150);
}

/** @param {import("playwright-core").Page} page */
async function statusPopup(page, which) {
  await page.waitForSelector(`[data-status="${which}"]`);
  await page.click(`[data-status="${which}"]`);
  await page.waitForSelector(".status-popup");
  await page.waitForTimeout(100);
}

/** @param {import("playwright-core").Page} page */
async function openFile(page, path) {
  await page.evaluate(async (p) => {
    const { openFileAt } = await import("/src/editor/views.ts");
    await openFileAt(p, 1);
  }, path);
  await page.waitForTimeout(300);
}

/** @type {Record<string, ((page: import("playwright-core").Page) => Promise<void>) & { clip?: unknown }>} */
const scenarios = {
  "status-line-separator": async (page) => statusPopup(page, "line-separator"),
  "status-encoding": async (page) => statusPopup(page, "encoding"),
  "status-indent": async (page) => statusPopup(page, "indent"),
  "settings-editor-general": async (page) => openSettingsPage(page, "editor.general"),
  "settings-code-style-php": async (page) => openSettingsPage(page, "editor.code-style.php"),
  "settings-inlay-hints": async (page) => openSettingsPage(page, "editor.inlay-hints"),
  "settings-git": async (page) => openSettingsPage(page, "vcs.git"),
  "settings-code-hosts": async (page) => openSettingsPage(page, "vcs.hosts"),
  "settings-system": async (page) => openSettingsPage(page, "system"),
  "editor-inlay-hints": async (page) => {
    await openFile(page, "app/Http/Controllers/OrderController.php");
    await page.waitForSelector(".cm-inlay-hint", { state: "attached" });
    await page.waitForTimeout(300);
  },
  "editor-code-vision": async (page) => {
    await openFile(page, "app/Http/Controllers/Controller.php");
    await page.waitForSelector(".cm-code-vision", { timeout: 15000 });
  },
  "vcs-commit-inspections": async (page) => {
    await page.keyboard.press("Control+k");
    await page.waitForSelector(".commit-row");
    await page.fill(
      ".commit-message textarea",
      "Apply every setting of the Settings dialog, from autosave to inlay hints and Git\nThe body starts right away without a blank line after the subject line.",
    );
    await page.waitForSelector("[data-commit-problems]");
  },
  "layout-restored": async (page) => {
    // Arrange: Problems open on its Project tab, a second file, a wider project view.
    await page.click('[data-tool-window="problems"]');
    await page.waitForSelector(".problems-tab");
    await page.click('.problems-tab:has-text("Project")');
    await openFile(page, "routes/web.php");
    await page.evaluate(async () => (await import("/src/state/layout.ts")).resizePanel("left", 380));
    await page.waitForTimeout(800);
    // Close the project and open it again from the Welcome screen.
    await page.evaluate(() => window.__forgeRunCommand("workspace.close"));
    await page.waitForSelector(".welcome-project");
    await page.click(".welcome-project >> nth=0");
    await page.waitForSelector(".problems-tab.active");
    await page.waitForSelector(".editor-tab.active, .cm-content", { state: "attached" });
    await page.waitForTimeout(500);
  },
  "ai-chat-history": async (page) => {
    await page.click('[data-tool-window="ai-chat"]');
    await page.waitForSelector(".chat-panel");
    await page.click(".chat-history .chat-dropdown-trigger");
    await page.waitForTimeout(200);
  },
};

// Startup balloons ("Database found in .env") would cover the status bar and the editor.
const quiet = Object.fromEntries(
  Object.entries(scenarios).map(([name, steps]) => [
    name,
    async (/** @type {import("playwright-core").Page} */ page) => {
      await page.addStyleTag({ content: ".balloons { display: none !important; }" });
      await steps(page);
    },
  ]),
);
for (const name of ["status-line-separator", "status-encoding", "status-indent"]) quiet[name].clip = statusClip;

export default quiet;
