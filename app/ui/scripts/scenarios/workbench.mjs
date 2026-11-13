/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "workbench-dark": async () => {},
  "workbench-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  },
};
