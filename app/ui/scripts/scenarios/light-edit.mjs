/**
 * Forge as the system's text editor: the LightEdit window for files opened with "Open With →
 * Forge" (`?dialog=light-edit&file=…` starts it as if those files were opened from the file
 * manager), and "Open Folder in Forge" while a project is open (`?openFolder=…`), which asks
 * This Window / New Window.
 */

const files = "file=/home/me/notes/todo.md&file=/etc/hosts&file=/home/me/Downloads/nginx.conf";

const lightEdit = async (page) => {
  await page.locator(".light-edit .cm-content:visible").first().waitFor();
  await page.waitForTimeout(300);
};
lightEdit.query = `?dialog=light-edit&${files}&windowControls=windows`;
lightEdit.waitFor = ".light-edit";

const lightEditLight = async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await lightEdit(page);
};
lightEditLight.query = lightEdit.query;
lightEditLight.waitFor = lightEdit.waitFor;

const lightEditEmpty = async (page) => {
  await page.waitForSelector(".light-edit-empty");
};
lightEditEmpty.query = "?dialog=light-edit&windowControls=gnome";
lightEditEmpty.waitFor = ".light-edit";

const openFolderPrompt = async (page) => {
  await page.waitForSelector(".popup-backdrop .dialog, .bc-confirm", { timeout: 15000 });
  await page.waitForTimeout(300);
};
openFolderPrompt.query = "?openFolder=~/Projects/symfony-api";

const openFolderFromLink = async (page) => {
  await page.waitForSelector(".popup-backdrop .dialog, .bc-confirm", { timeout: 15000 });
  await page.waitForTimeout(300);
};
openFolderFromLink.query = "?openFolder=~/Projects/symfony-api&fromLink";

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "light-edit-window": lightEdit,
  "light-edit-window-light": lightEditLight,
  "light-edit-empty": lightEditEmpty,
  "open-folder-window-prompt": openFolderPrompt,
  "open-folder-from-link": openFolderFromLink,
};
