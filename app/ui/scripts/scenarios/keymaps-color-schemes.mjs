/**
 * Settings | Keymap with the other bundled keymaps, user keymaps and mouse shortcuts, and
 * Settings | Editor | Color Scheme with a user scheme, attribute editing and the preview.
 */

/** @param {import("playwright-core").Page} page */
async function openKeymap(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.click(".set-tree-row:has-text('Keymap')");
  await page.waitForSelector(".keymap-page");
}

/** @param {import("playwright-core").Page} page */
async function openColorScheme(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click('.set-tree-row[data-node="editor.color-scheme"]');
  await page.waitForSelector(".csch-preview .csch-line");
  await page.waitForTimeout(300);
}

/** @param {import("playwright-core").Page} page @param {string} label */
async function gear(page, label) {
  await page.click(".csch-top .icon-btn[title='Show Scheme Actions'], .km-top .icon-btn[title='Show Keymap Actions']");
  await page.waitForSelector(".context-menu");
  await page.click(`.context-menu .context-menu-item:has-text("${label}")`);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "keymap-vscode": async (page) => {
    await openKeymap(page);
    await page.selectOption(".km-top select", "VSCode");
    await page.waitForSelector(".km-parent");
    await page.fill(".km-search input", "file");
    await page.waitForTimeout(300);
    await page.click(".km-action[data-action='GotoFile']");
  },
  "keymap-derive": async (page) => {
    await openKeymap(page);
    await page.selectOption(".km-top select", "Eclipse");
    await page.waitForSelector(".km-parent");
    await gear(page, "Derive");
    await page.waitForSelector(".dialog input");
    await page.fill(".dialog input", "My Eclipse");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.querySelector(".km-parent")?.textContent?.includes("Eclipse"));
    await page.click(".km-top .icon-btn[title='Show Keymap Actions']");
    await page.waitForSelector(".context-menu");
    await page.waitForTimeout(200);
  },
  "keymap-mouse-shortcut": async (page) => {
    await openKeymap(page);
    await page.fill(".km-search input", "Declaration or Usages");
    await page.waitForTimeout(300);
    await page.click(".km-action[data-action='GotoDeclaration']", { button: "right" });
    await page.click(".context-item:has-text('Add Mouse Shortcut')");
    await page.waitForSelector(".mouse-pad");
    await page.click(".mouse-pad", { modifiers: ["Control", "Alt"] });
    await page.waitForSelector(".conflict-row");
    await page.waitForTimeout(200);
  },
  "color-scheme-settings": async (page) => {
    await openColorScheme(page);
  },
  "color-scheme-edit": async (page) => {
    await openColorScheme(page);
    await page.selectOption("#csch-scheme", "Ember");
    await gear(page, "Duplicate");
    await page.waitForSelector(".dialog input");
    await page.fill(".dialog input", "Ember Bold");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".csch-editor input[type=checkbox]:not([disabled])");
    // Keyword: bold, a new color, wavy underline.
    await page.click(".csch-item:has-text('Keyword')");
    await page.check(".csch-font label:has-text('Bold') input");
    await page.waitForTimeout(200);
    await page.fill(".csch-color-row:has-text('Foreground') input[type=color]", "#ff5c8a");
    await page.waitForTimeout(200);
    await page.check(".csch-color-row:has-text('Effects') input[type=checkbox]");
    await page.waitForTimeout(200);
    await page.selectOption(".csch-effect select", "waveUnderline");
    await page.waitForTimeout(400);
  },
  "color-scheme-anvil": async (page) => {
    await openColorScheme(page);
    await page.selectOption("#csch-scheme", "Anvil");
    await page.waitForTimeout(300);
    await page.click(".csch-preview span:has-text('foreach')");
    await page.waitForTimeout(200);
  },
};
