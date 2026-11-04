/**
 * Screenshots for the user guide's shortcut pages and the main menu reference
 * (docs/guide/user/shortcuts/, docs/guide/reference/main-menu/). Every image is cropped to the
 * popup, dialog or tool window it documents.
 */

/** @param {import("playwright-core").Page} page @param {string} id */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/**
 * Crop to the union of all elements matching the selectors, with some padding, inside the
 * 1440×900 viewport.
 * @param {string[]} selectors @param {number} [pad]
 */
const clipTo =
  (selectors, pad = 12, { maxHeight = 0, extendUp = 0 } = {}) =>
  /** @param {import("playwright-core").Page} page */
  async (page) => {
    const boxes = [];
    for (const sel of selectors) {
      for (const el of await page.locator(sel).all()) {
        if (!(await el.isVisible())) continue;
        const b = await el.boundingBox();
        if (b && b.width > 0 && b.height > 0) boxes.push(b);
      }
    }
    if (!boxes.length) return undefined;
    const x = Math.max(0, Math.min(...boxes.map((b) => b.x)) - pad);
    const y = Math.max(0, Math.min(...boxes.map((b) => b.y)) - pad - extendUp);
    const right = Math.min(1440, Math.max(...boxes.map((b) => b.x + b.width)) + pad);
    let bottom = Math.min(900, Math.max(...boxes.map((b) => b.y + b.height)) + pad);
    if (maxHeight) bottom = Math.min(bottom, y + maxHeight);
    return { x, y, width: right - x, height: bottom - y };
  };

/** Balloons ("Database found in .env") would cover the documented UI. @param {import("playwright-core").Page} page */
const hideBalloons = (page) => page.addStyleTag({ content: ".balloons { display: none !important; }" });

/** Open files through Go to File so Recent Files has entries. @param {import("playwright-core").Page} page @param {string[]} names */
async function openFiles(page, names) {
  for (const name of names) {
    await page.keyboard.press("Control+Shift+KeyN");
    await page.waitForSelector(".se-popup");
    await page.keyboard.type(name);
    await page.waitForTimeout(300);
    await page.keyboard.press("Enter");
    await page.waitForTimeout(250);
  }
}

/** @param {import("playwright-core").Page} page */
async function openHamburger(page) {
  await page.click("[data-main-menu-button]");
  await page.waitForSelector(".context-menu.main-menu-popup");
}

/** Hover a row of the innermost open menu. @param {import("playwright-core").Page} page @param {string} label */
async function hover(page, label) {
  const exact = new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
  await page.locator(".context-menu-label", { hasText: exact }).last().hover();
  await page.waitForTimeout(120);
}

/** Put the caret into the open editor. @param {import("playwright-core").Page} page @param {number} line */
async function caret(page, line) {
  await page.click(`.editor-slot:not(.hidden) .cm-content .cm-line:nth-child(${line})`);
}

/** A scenario that opens a top-level menu (and optionally a submenu). */
function menu(top, sub) {
  /** @param {import("playwright-core").Page} page */
  const fn = async (page) => {
    await hideBalloons(page);
    await openHamburger(page);
    await hover(page, top);
    if (sub) await hover(page, sub);
  };
  fn.clip = clipTo([".context-menu"], 8);
  return fn;
}

/** A scenario that runs an action and crops to what it opened. */
function action(id, selectors, prepare, after, clipOptions) {
  /** @param {import("playwright-core").Page} page */
  const fn = async (page) => {
    await hideBalloons(page);
    if (prepare) await prepare(page);
    await run(page, id);
    await page.waitForSelector(selectors[0]);
    if (after) await after(page);
    await page.waitForTimeout(250);
  };
  fn.clip = clipTo(clipOptions?.selectors ?? selectors, 12, clipOptions);
  return fn;
}

/** A scenario that presses keys and crops to what they opened. */
function keys(combo, selectors, prepare, after, clipOptions) {
  /** @param {import("playwright-core").Page} page */
  const fn = async (page) => {
    await hideBalloons(page);
    if (prepare) await prepare(page);
    await page.keyboard.press(combo);
    await page.waitForSelector(selectors[0]);
    if (after) await after(page);
    await page.waitForTimeout(250);
  };
  fn.clip = clipTo(clipOptions?.selectors ?? selectors, 12, clipOptions);
  return fn;
}

const type = (text) => async (/** @type {import("playwright-core").Page} */ page) => {
  await page.keyboard.type(text);
  await page.waitForTimeout(350);
};
const panel = (id) => `[data-tool-window-panel="${id}"]`;
const editor = ".editor-slot:not(.hidden)";

/** @type {Record<string, ((page: import("playwright-core").Page) => Promise<void>) & { clip?: unknown }>} */
export default {
  // ---- Main menu: one image per top-level menu
  "docs-menu-file": menu("File", "New"),
  "docs-menu-edit": menu("Edit", "Find"),
  "docs-menu-view": menu("View", "Appearance"),
  "docs-menu-navigate": menu("Navigate"),
  "docs-menu-code": menu("Code", "Folding"),
  "docs-menu-refactor": menu("Refactor", "Extract/Introduce"),
  "docs-menu-run": menu("Run", "Debugging Actions"),
  "docs-menu-tools": menu("Tools", "Laravel"),
  "docs-menu-git": menu("Git", "Current File"),
  "docs-menu-window": menu("Window", "Editor Tabs"),
  "docs-menu-help": menu("Help"),

  // ---- Search and navigation
  "docs-search-everywhere": action("SearchEverywhere", [".se-popup"], undefined, type("user")),
  "docs-find-action": keys("Control+Shift+KeyA", [".se-popup"], undefined, type("reformat")),
  "docs-goto-class": keys("Control+KeyN", [".se-popup"], undefined, type("Order")),
  "docs-goto-file": keys("Control+Shift+KeyN", [".se-popup"], undefined, type("web")),
  "docs-goto-symbol": keys("Control+Alt+Shift+KeyN", [".se-popup"], undefined, type("total")),
  "docs-recent-files": keys("Control+KeyE", [".recent-popup"], (page) => openFiles(page, ["OrderController", "Checkout", "web.php"])),
  "docs-switcher": action("Switcher", [".recent-popup"], (page) => openFiles(page, ["OrderController", "Checkout", "web.php"])),
  "docs-find-in-files": keys("Control+Shift+KeyF", [".find-popup"], undefined, type("orders()")),
  "docs-find-in-file": keys("Control+KeyF", [`${editor} .cm-find-bar`], (page) => caret(page, 30), async (page) => {
    await page.fill(`${editor} .cm-find-bar textarea`, "orders");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(250);
  }, { selectors: [`${editor} .cm-panels-top`, `${editor} .cm-scroller`], maxHeight: 320 }),
  "docs-goto-line": keys("Control+KeyG", [`${editor} .cm-panels-bottom`], (page) => caret(page, 10), async (page) => {
    await page.keyboard.type("28");
  }, { extendUp: 160 }),

  // ---- Editing
  "docs-generate": keys("Alt+Insert", [".php-generate-popup"], (page) => caret(page, 27)),
  "docs-folding": keys("Control+Shift+Minus", [`${editor} .cm-scroller`], (page) => caret(page, 10), undefined, { maxHeight: 330 }),

  // ---- Run and debug
  "docs-run-chooser": keys("Alt+Shift+F10", [".run-chooser"]),
  "docs-run-configurations": Object.assign(
    async (/** @type {import("playwright-core").Page} */ page) => {
      await hideBalloons(page);
      await page.click(".run-config");
      await page.click(".run-dropdown .run-dd-item:has-text('Edit Configurations')");
      await page.waitForSelector(".rc-dialog");
      await page.click('.rc-tree-item[data-config="UserTest"]');
      await page.waitForTimeout(200);
    },
    { clip: clipTo([".wf-titlebar", ".rc-dialog"]) },
  ),
  "docs-view-breakpoints": keys("Control+Shift+F8", [".dbg-dialog"]),

  // ---- Git
  "docs-commit": keys("Control+KeyK", [panel("commit")]),
  "docs-branches": action("git.branches", [".branches-popup"]),
  "docs-git-log": keys("Alt+9", [panel("git")]),

  // ---- Tool windows
  "docs-structure": keys("Alt+7", [panel("structure")], undefined, undefined, { maxHeight: 260 }),
  "docs-problems": keys("Alt+6", [panel("problems")]),
  "docs-terminal": keys("Alt+F12", [panel("terminal")], undefined, async (page) => {
    await page.waitForTimeout(500);
  }),
  "docs-services": keys("Alt+8", [panel("services")]),

  // ---- General
  "docs-settings": keys("Control+Alt+KeyS", [".set-dialog"], undefined, undefined, { selectors: [".wf-titlebar", ".set-dialog"] }),
  "docs-new-scratch": keys("Control+Alt+Shift+Insert", [".scratch-popup"]),
  "docs-new-project": action("NewProject", [".np-dialog"], undefined, undefined, { selectors: [".wf-titlebar", ".np-dialog"] }),
  "docs-about": action("About", [".about-dialog"]),
  "docs-run-command": keys(
    "Control+Shift+KeyX",
    [".cli-popup .cli-alias"],
    undefined,
    async (page) => {
      await page.keyboard.type("a make:", { delay: 20 });
      await page.waitForSelector('.cli-item[data-cli-item="make:model"]');
    },
    { selectors: [".cli-popup", ".cli-popup-list"] },
  ),
};
