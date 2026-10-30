/**
 * Code With Me, joining as a guest: from a window that has its own project open in Safe
 * Mode, and from the Welcome screen (Remote Development > Code With Me). Browser mode
 * simulates the host with the demo project (`?project=cwm-join`); dark and light.
 */

const light = (page) => page.evaluate(() => (document.documentElement.dataset.theme = "light"));
const LINK = "forge://cwm/join?v=1&name=Robert&m=demo&s=browsermodedemo0000000000#h=demo&k=demo&a=127.0.0.1:41641";

/** Join Session… with the demo link; the host's project opens in this window. */
async function join(page, start = () => page.evaluate(() => void window.__forgeRunCommand("CodeWithMe.JoinSession"))) {
  await start();
  await page.waitForSelector('.dialog:has-text("Join Session") input');
  await page.fill('.dialog:has-text("Join Session") input', LINK);
  await page.click('.dialog:has-text("Join Session") button:has-text("Join")');
  await page.waitForSelector("[data-testid=cwm-status]");
  await page.waitForSelector('.tree-root:has-text("Robert (workstation)")');
}

/** The window's own project, untrusted: the Safe Mode banner and the expanded tree. */
const before = (theme) => async (page) => {
  await page.waitForSelector("[data-testid=safe-mode-banner]");
  await page.dblclick('.tree-row[data-path="app"]');
  if (theme === "light") await light(page);
};
before("dark").query = "?project=cwm-join";

/** After joining: the host's project in the Project view, no Safe Mode banner. */
const joined = (theme) => async (page) => {
  await page.waitForSelector("[data-testid=safe-mode-banner]");
  await page.dblclick('.tree-row[data-path="app"]');
  await join(page);
  await page.waitForSelector('.tree-row[data-path="app"]');
  if (await page.$("[data-testid=safe-mode-banner]")) throw new Error("the local project's Safe Mode banner is still shown");
  if (await page.$('.tree-row[data-path=".env"]')) throw new Error("the Project view still lists the local project's files");
  if (theme === "light") await light(page);
};

/** Welcome screen > Remote Development > Code With Me. */
const welcome = (theme) => async (page) => {
  if (theme === "light") await light(page);
};

/** Joined from the Welcome screen: the host's project opens in this window. */
const welcomeJoined = (theme) => async (page) => {
  await join(page, () => page.click("[data-testid=welcome-cwm-join]"));
  if (theme === "light") await light(page);
};

const scenarios = {
  "code-with-me-join-local": before("dark"),
  "code-with-me-join-joined": joined("dark"),
  "code-with-me-join-joined-light": joined("light"),
  "code-with-me-welcome": welcome("dark"),
  "code-with-me-welcome-light": welcome("light"),
  "code-with-me-welcome-joined": welcomeJoined("dark"),
  "code-with-me-welcome-joined-light": welcomeJoined("light"),
};
for (const [name, fn] of Object.entries(scenarios)) {
  if (name.startsWith("code-with-me-welcome")) {
    fn.query = "?project=cwm-join&welcome=codewithme";
    fn.waitFor = "[data-testid=welcome-code-with-me]";
  } else fn.query = "?project=cwm-join";
}

export default scenarios;
