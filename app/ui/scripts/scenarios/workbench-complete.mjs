/** Screenshots for split editors, editor tabs, Recent Locations, Bookmarks, TODO, Find and Notifications. */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openViaGotoFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForTimeout(300);
  await page.keyboard.press("Enter");
  await page.waitForTimeout(250);
}

/** @param {import("playwright-core").Page} page @param {string} id */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/** Close the startup balloons ("Database found in .env") so they do not cover the window. */
async function dismissBalloons(page) {
  await page.evaluate(() => document.querySelectorAll(".balloon .balloon-close").forEach((b) => b.click()));
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "split-editors": async (page) => {
    await openViaGotoFile(page, "OrderController");
    await run(page, "SplitVertically");
    await page.waitForSelector('[data-editor-group="2"] .editor-slot:not(.hidden) .cm-content');
    await openViaGotoFile(page, "Order.php");
    // Typing in one group shows up in the other one.
    await page.click('[data-editor-group="1"] .editor-slot:not(.hidden) .cm-content');
    await page.keyboard.press("Control+Home");
    await page.keyboard.type("// edited in the left group\n");
    await page.click('[data-editor-group="2"] .editor-tab[data-tab-id$="OrderController.php"]');
    await page.waitForTimeout(200);
  },
  "editor-tab-menu": async (page) => {
    await openViaGotoFile(page, "OrderController");
    await openViaGotoFile(page, "Order.php");
    await page.click('.editor-tab[data-tab-id$="OrderController.php"]', { button: "right" });
    await page.waitForSelector(".context-menu");
  },
  "pinned-tabs": async (page) => {
    await openViaGotoFile(page, "OrderController");
    await openViaGotoFile(page, "Order.php");
    await openViaGotoFile(page, "web.php");
    await page.click('.editor-tab[data-tab-id$="OrderController.php"]');
    await run(page, "PinActiveEditorTab");
    await page.click('.editor-tab[data-tab-id$="web.php"]');
    await run(page, "CloseAllEditorsButActive");
    await page.waitForTimeout(150);
  },
  "recent-locations": async (page) => {
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\n// TODO: check the full name");
    await openViaGotoFile(page, "OrderController");
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Control+End");
    await openViaGotoFile(page, "web.php");
    await page.keyboard.press("Control+Shift+E");
    await page.waitForSelector(".locations-popup .location-row");
  },
  bookmarks: async (page) => {
    await openViaGotoFile(page, "OrderController");
    await page.click(".editor-slot:not(.hidden) .cm-content");
    // F11 on line 25, Ctrl+Shift+2 on line 29, then the tool window (Alt+2).
    await page.keyboard.press("Control+Home");
    for (let i = 0; i < 24; i++) await page.keyboard.press("ArrowDown");
    await page.keyboard.press("F11");
    for (let i = 0; i < 4; i++) await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Control+Shift+Digit2");
    // Lines move with the text: two new lines above the bookmarks.
    await page.keyboard.press("Control+Home");
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(500);
    await page.keyboard.press("Alt+Digit2");
    await page.waitForSelector(".bookmark-row");
    await page.waitForTimeout(200);
  },
  "bookmark-mnemonic": async (page) => {
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Control+F11");
    await page.waitForSelector(".mnemonic-popup");
  },
  "find-window-text": async (page) => {
    await page.keyboard.press("Control+Shift+KeyF");
    await page.waitForSelector(".find-popup .find-input");
    await page.keyboard.type("total");
    await page.waitForSelector(".find-popup .find-row");
    await page.click(".find-open-window");
    await page.waitForSelector(".find-window .find-usage");
    // Next Occurrence (Ctrl+Alt+Down) walks the results.
    await run(page, "NextOccurence");
    await run(page, "NextOccurence");
    await page.waitForTimeout(300);
    await dismissBalloons(page);
  },
  notifications: async (page) => {
    // The demo project announces its database ("Database found in .env"); add a few more.
    await page.waitForSelector(".balloon");
    await page.evaluate(async () => {
      const { showBalloon } = await import("/src/keymap/dispatch.ts");
      showBalloon("Intelephense was installed", "Version 1.14.4 in Forge's data directory", 2600, undefined, { group: "Language Servers" });
      showBalloon("Xdebug is not configured", "PHP 8.3 has no xdebug extension loaded.", 0, [{ label: "Configure", run: () => {} }], {
        group: "Debugger",
        type: "warning",
      });
      showBalloon("Push failed", "rejected: main -> main (fetch first)", 2600, undefined, { group: "Version Control", type: "error" });
    });
    await run(page, "ActivateNotificationsToolWindow");
    await page.waitForSelector(".notif-entry");
    // Opened entries are read; the newest one stays unread.
    await page.click('.notif-entry:has-text("Push failed")');
    await page.click('.notif-entry:has-text("Intelephense")', { button: "right" });
    await page.waitForSelector(".context-menu");
    await page.hover('.context-menu-item:has-text("Display")');
    await page.waitForTimeout(250);
  },
  "editor-tabs-settings": async (page) => {
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("editor.tabs");
    });
    await page.waitForSelector(".set-dialog");
    await page.waitForTimeout(300);
  },
  todo: async (page) => {
    await openViaGotoFile(page, "OrderController");
    await run(page, "ActivateTODOToolWindow");
    await page.waitForSelector(".todo-item");
    // Live update: a new TODO typed in the editor shows up without saving.
    await page.click(".editor-slot:not(.hidden) .cm-content");
    await page.keyboard.press("Control+Home");
    await page.keyboard.press("End");
    await page.keyboard.type("\n// FIXME: check the order state first");
    await page.waitForSelector('.todo-item:has-text("check the order state")');
    await dismissBalloons(page);
  },
};
