const welcome = async () => {};
welcome.query = "?welcome";
welcome.waitFor = ".welcome-project";

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  welcome,
};
