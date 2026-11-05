/**
 * The overlay stack (shell/layers.tsx): whatever opens from a dialog or a floating tool window
 * is above it, and notifications stay above dialogs. Every case in the dark and light theme.
 */

/** @param {import("playwright-core").Page} page */
async function light(page) {
  await page.evaluate(async () => {
    const { saveSetting } = await import("/src/state/settings.ts");
    await saveSetting("appearance.theme", "light");
  });
  await page.waitForTimeout(300);
}

/** Settings › Keymap. */
async function keymapSettings(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click('.set-tree-row[data-node="keymap"]');
  await page.waitForSelector(".keymap-page");
}

/** The keymap's gear menu: a context menu opened from the modal Settings dialog. */
async function contextMenuInDialog(page) {
  await keymapSettings(page);
  await page.click(".km-top .icon-btn");
  await page.waitForSelector('[data-layer-kind="context-menu"] .context-menu');
}

/** Evaluate Expression (Alt+F8) in a paused session, with its completion list open. */
async function completionInDialog(page) {
  for (const dir of ["app/Http", "app/Http/Controllers"]) await page.click(`.tree-row[data-path="${dir}"]`);
  await page.click('.tree-row[data-path="app/Http/Controllers/OrderController.php"]');
  await page.waitForSelector(".cm-bp-dot");
  await page.click(".dbg-listen-btn");
  await page.waitForSelector(".cm-exec-line");
  await page.waitForSelector('.dbg-row[data-var="$order"]');
  await page.locator(".cm-content:visible").click({ position: { x: 600, y: 10 } });
  await page.keyboard.press("Alt+F8");
  await page.waitForSelector(".dbg-evaluate #dbg-eval-expr");
  await page.fill(".dbg-evaluate #dbg-eval-expr", "");
  await page.locator(".dbg-evaluate #dbg-eval-expr").pressSequentially("$o", { delay: 40 });
  await page.waitForSelector(".dbg-evaluate .dbg-completions");
  // Focus scrolls the page sideways when the status bar is wider than the window.
  await page.evaluate(() => (document.documentElement.scrollLeft = 0));
}

/** A balloon that arrives while Settings is open. */
async function notificationOverDialog(page) {
  await keymapSettings(page);
  await page.evaluate(async () => {
    const { showBalloon } = await import("/src/keymap/index.ts");
    showBalloon("Indexing finished", "Notifications stay above dialogs.", 60000);
  });
  await page.waitForSelector(".balloon");
}

/** The Project tool window floating over the editor, with its options menu open. */
async function popupFromFloatingToolWindow(page) {
  await page.evaluate(async () => {
    const { setToolWindowViewMode } = await import("/src/state/layout.ts");
    setToolWindowViewMode("project", "float");
  });
  await page.waitForSelector('.floating-tool-window[data-floating-tool-window="project"]');
  await page.click('.floating-tool-window[data-floating-tool-window="project"] [data-tool-window-options]');
  await page.waitForSelector('[data-layer-kind="context-menu"] .context-menu');
  // The View Mode submenu, which reaches out of the floating window over the editor.
  await page.hover('.context-menu-item:has-text("View Mode")');
  await page.waitForSelector(".context-menu .context-menu");
}

/** Settings › Keymap › Duplicate…: a dialog opened from a dialog. */
async function nestedDialogs(page) {
  await contextMenuInDialog(page);
  await page.click('.context-menu-item:has-text("Duplicate")');
  await page.waitForSelector('[data-layer-kind="modal"] .popup-backdrop.modal input');
}

const withLight = (steps) => async (page) => {
  await light(page);
  await steps(page);
};

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "layers-context-menu-in-dialog": contextMenuInDialog,
  "layers-context-menu-in-dialog-light": withLight(contextMenuInDialog),
  "layers-completion-in-dialog": completionInDialog,
  "layers-completion-in-dialog-light": withLight(completionInDialog),
  "layers-notification-over-dialog": notificationOverDialog,
  "layers-notification-over-dialog-light": withLight(notificationOverDialog),
  "layers-floating-tool-window-popup": popupFromFloatingToolWindow,
  "layers-floating-tool-window-popup-light": withLight(popupFromFloatingToolWindow),
  "layers-nested-dialogs": nestedDialogs,
  "layers-nested-dialogs-light": withLight(nestedDialogs),
};
