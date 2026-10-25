/**
 * Workbench windows: Distraction-Free and Presentation Mode, a floating tool window (View
 * Mode › Float), Window | Layouts with a saved layout, the zoomed IDE and the Open Project
 * question (New Window / This Window).
 */

/** @param {import("playwright-core").Page} page @param {string} id */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/** @param {import("playwright-core").Page} page */
async function openHamburger(page) {
  await page.click("[data-main-menu-button]");
  await page.waitForSelector(".context-menu.main-menu-popup");
}

/** Hover a row of the innermost open menu. @param {import("playwright-core").Page} page @param {string} label */
async function hover(page, label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".context-menu-label", { hasText: exact }).last().hover();
  await page.waitForTimeout(80);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "distraction-free-mode": async (page) => {
    await page.click('[data-tool-window="terminal"]');
    await run(page, "ToggleDistractionFreeMode");
    await page.waitForSelector("html[data-appearance-mode='distractionFree']");
    if (await page.locator(".main-toolbar:visible, .tool-stripe:visible").count()) throw new Error("bars are still visible");
    await page.click(".cm-content .cm-line:nth-child(12)");
  },

  "distraction-free-restores-layout": async (page) => {
    await page.click('[data-tool-window="terminal"]');
    await run(page, "ToggleDistractionFreeMode");
    await page.waitForSelector("html[data-appearance-mode='distractionFree']");
    // The View | Appearance entry now reads "Exit …"; leaving brings the Terminal back.
    await run(page, "ToggleDistractionFreeMode");
    await page.waitForSelector('[data-tool-window-panel="terminal"]');
    await openHamburger(page);
    await hover(page, "View");
    await hover(page, "Appearance");
  },

  "presentation-mode": async (page) => {
    await run(page, "TogglePresentationMode");
    await page.waitForSelector("html[data-appearance-mode='presentation']");
    await page.click(".cm-content .cm-line:nth-child(8)");
  },

  "floating-tool-window": async (page) => {
    await page.click('[data-tool-window="terminal"]');
    await page.waitForSelector('[data-tool-window-panel="terminal"]');
    await page.click('[data-tool-window-panel="terminal"] [data-tool-window-options]');
    await page.hover('.context-menu-item:has-text("View Mode")');
    await page.getByRole("menuitemcheckbox", { name: "Float" }).click();
    await page.waitForSelector('[data-floating-tool-window="terminal"]');
    // Move it by its header.
    const header = await page.locator('[data-floating-tool-window="terminal"] .tool-window-header').boundingBox();
    await page.mouse.move(header.x + 120, header.y + header.height / 2);
    await page.mouse.down();
    await page.mouse.move(header.x - 60, header.y - 260, { steps: 8 });
    await page.mouse.up();
    // The header's ⋮ menu shows the current view mode.
    await page.click('[data-floating-tool-window="terminal"] [data-tool-window-options]');
    await page.hover('.context-menu-item:has-text("View Mode")');
    await page.getByRole("menuitemcheckbox", { name: "Dock Pinned" }).waitFor();
  },

  "layouts-menu": async (page) => {
    await page.click('[data-tool-window="terminal"]');
    await page.evaluate(async () => {
      const { projectStateApi } = await import("/src/ipc/projectState.ts");
      const { windowLayout } = await import("/src/state/layoutPersistence.ts");
      await projectStateApi.saveNamedLayout("Debugging", windowLayout(), true);
      await projectStateApi.saveNamedLayout("Code Review", { active: { right: "commit" }, sizes: {}, tabs: {}, openFiles: [], activeFile: null }, true);
    });
    await openHamburger(page);
    await hover(page, "Window");
    await hover(page, "Layouts");
  },

  "save-layout-dialog": async (page) => {
    // The command waits for the dialog's answer: do not wait for it.
    await page.evaluate(() => void window.__forgeRunCommand("StoreNewLayout"));
    await page.waitForSelector(".dialog-title:has-text('Save Layout')");
    await page.keyboard.type("Database Work");
  },

  "zoomed-ide": async (page) => {
    await run(page, "ZoomInIdeAction");
    await run(page, "ZoomInIdeAction");
    await page.waitForFunction(() => document.documentElement.style.zoom === "1.25");
    await page.click('[data-tool-window="problems"]');
  },

  "open-project-new-window": async (page) => {
    await openHamburger(page);
    await hover(page, "File");
    await hover(page, "Recent Projects");
    await page.locator(".context-menu-item", { hasText: "symfony-api" }).last().click();
    await page.waitForSelector(".dialog-title:has-text('Open Project')");
  },
};
