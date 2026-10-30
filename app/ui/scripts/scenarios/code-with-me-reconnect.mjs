/**
 * Code With Me: a dropped connection (guest and host side), the Join Session dialog with its
 * verification code, and a terminal the host stopped sharing.
 */
const light = (page) => page.evaluate(() => (document.documentElement.dataset.theme = "light"));
const LINK = "forge://cwm/join?v=1&name=Robert&m=workstation&s=browsermodedemo0000000000#h=demo&k=demo&a=192.168.1.20:41641";

/** The guest's window while Forge restores the connection to the host. */
const reconnecting = (theme) => {
  const fn = async (page) => {
    await page.waitForSelector("[data-testid=cwm-status].reconnecting");
    if (theme === "light") await light(page);
    await page.waitForSelector('.balloon:has-text("Connection to")');
    await page.click("[data-testid=cwm-status]");
    await page.waitForSelector(".cwm-status-popup");
    await page.waitForTimeout(300);
  };
  fn.query = "?project=cwm-guest&permission=edit&reconnecting";
  fn.waitFor = "[data-testid=cwm-status]";
  return fn;
};

/** The host sees Alex reconnecting in the participant list. */
const hostSeesReconnecting = (theme) => {
  const fn = async (page) => {
    if (theme === "light") await light(page);
    await page.click(".cwm-toolbar-btn");
    await page.waitForSelector(".cwm-popup");
    await page.click('.cwm-popup button:has-text("Start Session")');
    await page.waitForSelector('.balloon:has-text("wants to join the session")');
    await page.click('.balloon:has-text("wants to join the session") button:has-text("Accept")');
    await page.waitForSelector(".cwm-toolbar-avatar");
    await page.click(".cwm-toolbar-btn");
    await page.waitForSelector(".cwm-popup");
    await page.waitForSelector(".cwm-participant.reconnecting", { timeout: 15000 });
    await page.waitForTimeout(300);
  };
  fn.query = "?cwmAway";
  fn.clip = async (page) => {
    const box = await page.locator(".cwm-popup").boundingBox();
    return { x: Math.max(0, box.x - 380), y: 0, width: Math.min(1440, box.width + 420), height: Math.min(900, box.y + box.height + 20) };
  };
  return fn;
};

/** Join Session… with a pasted link: the host, its machine and the name to join with. */
const joinDialog = (theme) => {
  const fn = async (page) => {
    if (theme === "light") await light(page);
    await page.evaluate(() => void window.__forgeRunCommand("CodeWithMe.JoinSession"));
    await page.waitForSelector(".cwm-join-dialog input");
    await page.fill(".cwm-join-dialog input", LINK);
    await page.waitForSelector("[data-testid=cwm-join-summary]");
    await page.waitForTimeout(200);
  };
  fn.clip = { x: 420, y: 160, width: 600, height: 460 };
  return fn;
};

/** Join pressed: waiting for the host, with the verification code to compare. */
const joinWaiting = (theme) => {
  const fn = joinDialog(theme);
  const run = async (page) => {
    await fn(page);
    await page.click('.cwm-join-dialog button:has-text("Join")');
    await page.waitForSelector("[data-testid=cwm-join-code]");
    await page.waitForTimeout(200);
  };
  run.query = "?cwmHostWaits";
  run.clip = fn.clip;
  return run;
};

/** Join with unsaved changes in the window's own project: Save, Don't Save or Cancel first. */
const joinUnsaved = (theme) => {
  const fn = joinDialog(theme);
  const run = async (page) => {
    await page.click(".cm-content");
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\n// draft");
    await fn(page);
    await page.click('.cwm-join-dialog button:has-text("Join")');
    await page.waitForSelector("[data-testid=cwm-join-unsaved]");
    await page.waitForTimeout(200);
  };
  run.clip = fn.clip;
  return run;
};

/** Forge was opened by clicking a forge://cwm/join?… link: the Welcome screen shows the dialog. */
const joinFromLink = (theme) => {
  const fn = async (page) => {
    if (theme === "light") await light(page);
    await page.waitForSelector("[data-testid=cwm-join-summary]");
    await page.waitForTimeout(300);
  };
  fn.query = "?welcome&cwmLink";
  fn.waitFor = ".cwm-join-dialog";
  return fn;
};

/** The host stopped sharing its terminal: the guest's tab stays with what it showed. */
const unsharedTerminal = (theme) => {
  const fn = async (page) => {
    await page.waitForSelector("[data-testid=cwm-status]");
    if (theme === "light") await light(page);
    await page.keyboard.press("Alt+F12");
    await page.waitForSelector('.term-tab[data-terminal-tab="Local"]');
    await page.waitForSelector(".term-host:not(.hidden) .xterm-screen");
    // The demo host stops sharing two seconds after the tab attached (the text is drawn on a canvas).
    await page.waitForTimeout(3500);
  };
  fn.query = "?project=cwm-guest&permission=readOnly&share&unshare";
  fn.waitFor = "[data-testid=cwm-status]";
  return fn;
};

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "cwm-guest-reconnecting": reconnecting("dark"),
  "cwm-guest-reconnecting-light": reconnecting("light"),
  "code-with-me-guest-reconnecting": hostSeesReconnecting("dark"),
  "code-with-me-guest-reconnecting-light": hostSeesReconnecting("light"),
  "code-with-me-join-dialog": joinDialog("dark"),
  "code-with-me-join-dialog-light": joinDialog("light"),
  "code-with-me-join-waiting": joinWaiting("dark"),
  "code-with-me-join-waiting-light": joinWaiting("light"),
  "code-with-me-join-unsaved": joinUnsaved("dark"),
  "code-with-me-join-unsaved-light": joinUnsaved("light"),
  "code-with-me-join-link": joinFromLink("dark"),
  "code-with-me-join-link-light": joinFromLink("light"),
  "cwm-guest-terminal-unshared": unsharedTerminal("dark"),
  "cwm-guest-terminal-unshared-light": unsharedTerminal("light"),
};
