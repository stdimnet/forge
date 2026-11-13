/**
 * Forge's own window controls per platform style (forced with `?windowControls=<style>`; in
 * browser mode "auto" leaves the frame to the browser) and dialogs in their window frame.
 */

const openSettings = async (page) => {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".wf .set-tree-row");
};

/** @param {string} style @param {(page: import("playwright-core").Page) => Promise<void>} [hover] */
function titlebar(style, hover) {
  const fn = async (page) => {
    await page.waitForSelector(`.main-toolbar [data-window-controls="${style}"]`);
    if (hover) await hover(page);
  };
  fn.query = `?windowControls=${style}`;
  return fn;
}

/** @param {string} style */
function settingsIn(style) {
  const fn = async (page) => {
    await openSettings(page);
    await page.click('.set-tree-row[data-node="appearance"]');
    await page.waitForTimeout(200);
  };
  fn.query = `?windowControls=${style}`;
  return fn;
}

const lightGnome = titlebar("gnome", async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await page.hover('.main-toolbar .wc-close');
});
lightGnome.query = "?windowControls=gnome";

/** The page a dialog window loads: only the dialog, filling the window. */
const dialogWindow = async (page) => {
  await page.waitForSelector(".set-tree-row");
  await page.hover(".wf-titlebar .wc-close");
};
dialogWindow.query = "?dialog=settings&windowControls=kde";
dialogWindow.waitFor = ".wf-window .set-dialog";

const welcome = async (page) => {
  await page.waitForSelector(".welcome-window .wf-titlebar");
};
welcome.query = "?welcome&windowControls=macos";
welcome.waitFor = ".welcome";

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "titlebar-windows": titlebar("windows", (page) => page.hover(".main-toolbar .wc-close")),
  "titlebar-macos": titlebar("macos", (page) => page.hover(".main-toolbar .wc-maximize")),
  "titlebar-gnome": titlebar("gnome", (page) => page.hover(".main-toolbar .wc-close")),
  "titlebar-kde": titlebar("kde", (page) => page.hover(".main-toolbar .wc-close")),
  "titlebar-linux": titlebar("linux", (page) => page.hover(".main-toolbar .wc-maximize")),
  "titlebar-gnome-light": lightGnome,
  "settings-window-windows": settingsIn("windows"),
  "settings-window-macos": settingsIn("macos"),
  "settings-window-gnome": settingsIn("gnome"),
  "settings-window-kde": settingsIn("kde"),
  "settings-dialog-window-kde": dialogWindow,
  "welcome-titlebar-macos": welcome,
};
