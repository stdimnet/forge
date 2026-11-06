/**
 * Code With Me: the toolbar popup, a session with the scripted guest "Alex" (remote caret,
 * label and selection), the session popup, Settings | Tools | Code With Me, and two pages
 * (two browser contexts) editing the same file.
 */

/** The visible editor (inactive tabs keep their hidden editors). */
const ACTIVE = `[...document.querySelectorAll(".cm-content")].find((c) => c.offsetParent !== null)`;

/** The visible editor's text without the remote carets' name labels. */
const TEXT = `(() => {
  const c = ${ACTIVE};
  if (!c) return "";
  return [...c.querySelectorAll(".cm-line")].map((l) => {
    const k = l.cloneNode(true);
    k.querySelectorAll(".cm-ySelectionCaret").forEach((e) => e.remove());
    return k.textContent;
  }).join("\\n");
})()`;

const light = (page) => page.evaluate(() => (document.documentElement.dataset.theme = "light"));

/** Open OrderController.php from the project tree (single clicks). */
async function openController(page) {
  await page.waitForSelector(".tree-row");
  for (const dir of ["app/Http", "app/Http/Controllers"]) {
    if (!(await page.$(`.tree-row[data-path="${dir}"]`))) await page.click(`.tree-row[data-path="${dir.split("/").slice(0, -1).join("/")}"]`);
    await page.waitForSelector(`.tree-row[data-path="${dir}"]`);
    if (!(await page.$(`.tree-row[data-path^="${dir}/"]`))) await page.click(`.tree-row[data-path="${dir}"]`);
  }
  await page.click('.tree-row[data-path="app/Http/Controllers/OrderController.php"]');
  await page.waitForFunction(`${ACTIVE}?.textContent.includes("OrderController")`);
}

async function openPopup(page) {
  await page.click(".cwm-toolbar-btn");
  await page.waitForSelector(".cwm-popup");
}

/** The host's approval balloon names the guest and the code; accept it. */
async function acceptGuest(page) {
  await page.waitForSelector('.balloon:has-text("wants to join the session")');
  await page.click('.balloon:has-text("wants to join the session") button:has-text("Accept")');
}

/** Start a session; Alex joins (the host accepts), types a comment and selects the `if` block. */
async function sessionWithAlex(page) {
  await openController(page);
  await openPopup(page);
  await page.click('.cwm-level:has-text("Edit files") input');
  await page.click('.cwm-popup button:has-text("Start Session")');
  await acceptGuest(page);
  await page.waitForFunction(`${ACTIVE}?.textContent.includes("// VIP customers")`, null, { timeout: 15000 });
  await page.waitForSelector(".cm-ySelection");
  await page.waitForTimeout(400);
  // Our own caret elsewhere, so both carets show.
  await page.click('.cm-line:visible:has-text("$this->authorize")');
}

const popupClip = async (page) => {
  const box = await page.locator(".cwm-popup").boundingBox();
  return { x: Math.max(0, box.x - 380), y: 0, width: Math.min(1440, box.width + 420), height: Math.min(900, box.y + box.height + 20) };
};

const editorClip = { x: 340, y: 40, width: 1060, height: 700 };

const startPopup = async (page) => openPopup(page);
startPopup.clip = popupClip;
const startPopupLight = async (page) => {
  await light(page);
  await openPopup(page);
};
startPopupLight.clip = popupClip;

const carets = async (page) => sessionWithAlex(page);
carets.clip = editorClip;
const caretsLight = async (page) => {
  await light(page);
  await sessionWithAlex(page);
};
caretsLight.clip = editorClip;
/** Start a Full access session that Alex joins, and open the Terminal tool window. */
async function fullSessionWithTerminal(page) {
  await openPopup(page);
  await page.click('.cwm-level:has-text("Full access") input');
  await page.click('.cwm-popup button:has-text("Start Session")');
  await acceptGuest(page);
  await page.keyboard.press("Alt+F12");
  await page.waitForSelector('.term-tab[data-terminal-tab="Local"]');
  await page.waitForSelector(".term-host:not(.hidden) .xterm-screen");
  await page.waitForTimeout(300);
}

/** The host's terminal tab menu with Share with Guests (S5). */
const shareTerminalMenu = (theme) => async (page) => {
  if (theme === "light") await light(page);
  await fullSessionWithTerminal(page);
  await page.click('.term-tab[data-terminal-tab="Local"]', { button: "right" });
  await page.waitForSelector('.run-popup-menu :text("Share with Guests")');
};

/** Alex opened a terminal on the host's machine and typed into it; the host's Local tab is shared. */
const guestTerminalOnHost = (theme) => async (page) => {
  if (theme === "light") await light(page);
  await fullSessionWithTerminal(page);
  await page.click('.term-tab[data-terminal-tab="Local"]', { button: "right" });
  await page.click('.run-popup-menu :text("Share with Guests")');
  await page.evaluate(() => window.__forgeCollabMock.guestTerminal("Alex"));
  await page.click('.term-tab[data-terminal-tab="Alex: Local"]');
  await page.waitForTimeout(900);
};

/** Two seconds after Alex stopped, the name label has faded; the stripe mark on the right stays. */
const caretsIdle = (theme) => {
  const fn = async (page) => {
    if (theme === "light") await light(page);
    await sessionWithAlex(page);
    await page.waitForSelector(".cm-stripe-collab-2");
    await page.waitForTimeout(2600);
  };
  fn.clip = editorClip;
  return fn;
};

const sessionPopup = async (page) => {
  await sessionWithAlex(page);
  await openPopup(page);
};
sessionPopup.clip = popupClip;
const sessionPopupLight = async (page) => {
  await light(page);
  await sessionWithAlex(page);
  await openPopup(page);
};
sessionPopupLight.clip = popupClip;

async function settingsPage(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click('.set-tree-row[data-node="tools.code-with-me"]');
  await page.waitForSelector('[data-setting="codeWithMe.userName"]');
}
const settings = async (page) => settingsPage(page);
const settingsLight = async (page) => {
  await light(page);
  await settingsPage(page);
};

/** Tools | Code With Me in the main menu. */
async function toolsMenu(page) {
  const hover = async (label) => {
    await page.locator(".context-menu-label", { hasText: new RegExp(`^${label}$`) }).last().hover();
    await page.waitForTimeout(100);
  };
  await page.click("[data-main-menu-button]");
  await page.waitForSelector(".context-menu.main-menu-popup");
  await hover("Tools");
  await hover("Code With Me");
}
const menu = async (page) => toolsMenu(page);
menu.clip = { x: 0, y: 0, width: 760, height: 560 };
const menuLight = async (page) => {
  await light(page);
  await toolsMenu(page);
};
menuLight.clip = menu.clip;

/**
 * Two browser contexts: `?collab=host` and `?collab=guest`, bridged through the mock's
 * wire. Both type into OrderController.php; the texts must converge and each page must draw
 * the other's caret. The screenshot shows the host page.
 */
async function twoPages(page, theme) {
  const url = new URL(page.url());
  const other = await page.context().browser().newContext({ viewport: { width: 1440, height: 900 } });
  const guest = await other.newPage();
  const relay = (to) => (msg) => to.evaluate((m) => window.__cwmReceive?.(m), msg).catch(() => undefined);
  await page.exposeFunction("__cwmOut", relay(guest));
  await guest.exposeFunction("__cwmOut", relay(page));
  await page.goto(`${url.origin}/?collab=host`);
  await page.waitForSelector(".cm-content");
  if (theme === "light") await light(page);
  await openController(page);
  await guest.goto(`${url.origin}/?collab=guest`);
  await acceptGuest(page);
  await guest.waitForFunction(`${ACTIVE}?.textContent.includes("OrderController")`, null, { timeout: 15000 });
  if (theme === "light") await light(guest);
  // The guest types a comment at the end of the discount line, the host above `return`.
  await guest.click('.cm-line:visible:has-text("$discount = $user->isVip()")');
  await guest.keyboard.press("End");
  await guest.keyboard.type(" // VIP customers", { delay: 20 });
  await page.click('.cm-line:visible:has-text("return response()->json")');
  await page.keyboard.press("Home");
  await page.keyboard.type("// Respond with the order and its total.\n        ", { delay: 10 });
  // The guest selects the payable block; the host sees it in the guest's color.
  await guest.click('.cm-line:visible:has-text("if ($order->isPayable())")');
  await guest.keyboard.press("Home");
  await guest.keyboard.down("Shift");
  for (let i = 0; i < 3; i++) await guest.keyboard.press("ArrowDown");
  await guest.keyboard.up("Shift");
  const text = (p) => p.evaluate(TEXT);
  for (let i = 0; i < 50; i++) {
    const [a, b] = [await text(page), await text(guest)];
    if (a === b && a.includes("// VIP customers") && a.includes("// Respond with")) break;
    if (i === 49) throw new Error("the two pages did not converge");
    await page.waitForTimeout(100);
  }
  await page.waitForSelector(".cm-ySelectionCaret");
  await guest.waitForSelector(".cm-ySelectionCaret");
  await page.waitForSelector(".cm-ySelection");
  // Undo in the guest reverts only the guest's typing.
  await guest.click('.cm-line:visible:has-text("$this->authorize")');
  await guest.keyboard.press("Control+KeyZ");
  for (let i = 0; i < 30; i++) {
    const a = await text(page);
    if (!a.includes("// VIP customers") && a.includes("// Respond with")) break;
    if (i === 29) throw new Error("undo did not revert only the guest's edit");
    await page.waitForTimeout(100);
  }
  await guest.keyboard.press("Control+Shift+KeyZ");
  await page.waitForFunction(`${TEXT}.includes("// VIP customers")`);
  await guest.click('.cm-line:visible:has-text("if ($order->isPayable())")');
  await guest.keyboard.press("Home");
  await guest.keyboard.down("Shift");
  for (let i = 0; i < 3; i++) await guest.keyboard.press("ArrowDown");
  await guest.keyboard.up("Shift");
  await page.waitForSelector(".cm-ySelection");
  await page.waitForTimeout(300);
  page.once("close", () => void other.close());
  twoPages.guest = guest;
}

const twoPagesDark = async (page) => twoPages(page, "dark");
twoPagesDark.clip = editorClip;
const twoPagesLight = async (page) => twoPages(page, "light");
twoPagesLight.clip = editorClip;

/** The host's balloon asks about Alex: the code, the permissions and Accept / Decline. */
async function approvalBalloon(page) {
  await openController(page);
  await openPopup(page);
  await page.click('.cwm-popup button:has-text("Start Session")');
  await page.waitForSelector('.balloon:has-text("wants to join the session")');
  await page.waitForTimeout(300);
}
const approval = async (page) => approvalBalloon(page);
approval.clip = { x: 1000, y: 540, width: 440, height: 360 };
const approvalLight = async (page) => {
  await light(page);
  await approvalBalloon(page);
};
approvalLight.clip = approval.clip;

/** The host's menu for Alex in the participant list, with the Permissions submenu open. */
async function guestMenu(page) {
  await sessionWithAlex(page);
  await openPopup(page);
  await page.click('.cwm-participant:has-text("Alex") .cwm-participant-menu');
  await page.waitForSelector('.context-menu .context-menu-item:has-text("Remove Guest")');
  await page.hover('.context-menu .context-menu-item:has-text("Permissions")');
  await page.waitForTimeout(500);
  await page.waitForTimeout(200);
}
const guestMenuDark = async (page) => guestMenu(page);
guestMenuDark.clip = { x: 700, y: 0, width: 740, height: 420 };
const guestMenuLight = async (page) => {
  await light(page);
  await guestMenu(page);
};
guestMenuLight.clip = guestMenuDark.clip;

/** Start Session with Custom…: the Custom Permissions dialog. */
async function customDialog(page) {
  await openPopup(page);
  await page.click('.cwm-level[data-level="custom"] input');
  await page.waitForSelector(".cwm-custom-dialog");
  await page.selectOption('.cwm-custom-dialog select[data-area="files"]', "edit");
  await page.selectOption('.cwm-custom-dialog select[data-area="vcs"]', "full");
}
const custom = async (page) => customDialog(page);
custom.clip = { x: 420, y: 200, width: 600, height: 420 };
const customLight = async (page) => {
  await light(page);
  await customDialog(page);
};
customLight.clip = custom.clip;

/** The values chosen, back in the Start Session popup. */
const customChosen = async (page) => {
  await customDialog(page);
  await page.click('.cwm-custom-dialog button:has-text("OK")');
  await page.waitForSelector('.cwm-level[data-level="custom"].selected');
};
customChosen.clip = popupClip;

/** The Session Log after Alex joined and got Full access (menus driven by the keyboard). */
async function sessionLog(page) {
  await sessionWithAlex(page);
  await openPopup(page);
  await page.click('.cwm-participant:has-text("Alex") .cwm-participant-menu');
  await page.waitForSelector('.context-menu .context-menu-item:has-text("Remove Guest")');
  // Follow, Jump To, then Permissions ▸ Full access.
  for (const key of ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowRight", "ArrowDown", "ArrowDown", "Enter"]) await page.keyboard.press(key);
  // The popup stays open around its menus.
  await page.waitForSelector('.cwm-participant:has-text("Alex") .cwm-participant-perm:has-text("Full access")');
  await page.click('.cwm-menu-item:has-text("Session Log…")');
  await page.waitForSelector(".cwm-log-line");
  await page.waitForTimeout(300);
}
const log = async (page) => sessionLog(page);
log.clip = { x: 380, y: 160, width: 680, height: 560 };
const logLight = async (page) => {
  await light(page);
  await sessionLog(page);
};
logLight.clip = log.clip;

/** The host follows Alex (toolbar avatar): the banner above the editor and Alex's position. */
async function followAlex(page) {
  await sessionWithAlex(page);
  await page.click('.cwm-toolbar-avatar[data-participant="1"]');
  await page.waitForSelector(".editor-slot:not(.hidden) .cwm-follow-banner:not([hidden])");
  await page.waitForTimeout(400);
}
const following = async (page) => followAlex(page);
const followingLight = async (page) => {
  await light(page);
  await followAlex(page);
};

/** The Code With Me tool window: Alex's direct path, file and line, Make Everyone Follow You on. */
async function toolWindow(page) {
  await sessionWithAlex(page);
  await page.evaluate(() => window.__forgeRunCommand("CodeWithMe.MakeEveryoneFollow"));
  await page.evaluate(() => window.__forgeRunCommand("CodeWithMe.ShowToolWindow"));
  await page.waitForSelector('.cwm-tool-window .cwm-participant:has-text("Alex") .cwm-participant-where');
  await page.waitForSelector('.cwm-tool-window .cwm-participant:has-text("Alex") .cwm-path[data-path="direct"]');
  await page.waitForTimeout(300);
}
const tool = async (page) => toolWindow(page);
const toolLight = async (page) => {
  await light(page);
  await toolWindow(page);
};

/** Alex's menu: Follow, Jump To and the host's guest actions. */
async function followMenu(page) {
  await sessionWithAlex(page);
  await openPopup(page);
  await page.click('.cwm-participant:has-text("Alex") .cwm-participant-menu');
  await page.waitForSelector('.context-menu .context-menu-item:has-text("Jump To")');
  await page.waitForTimeout(200);
}
const followMenuDark = async (page) => followMenu(page);
followMenuDark.clip = guestMenuDark.clip;
const followMenuLight = async (page) => {
  await light(page);
  await followMenu(page);
};
followMenuLight.clip = guestMenuDark.clip;

/** The host's popup with Make Everyone Follow You on. */
async function everyone(page) {
  await sessionWithAlex(page);
  await page.evaluate(() => window.__forgeRunCommand("CodeWithMe.MakeEveryoneFollow"));
  await openPopup(page);
  await page.waitForSelector('.cwm-check-item[aria-checked="true"]');
}
const everyoneDark = async (page) => everyone(page);
everyoneDark.clip = popupClip;
const everyoneLight = async (page) => {
  await light(page);
  await everyone(page);
};
everyoneLight.clip = popupClip;

/** The lines Alex changed have a marker in Alex's color in the host's gutter (until saved). */
const guestLines = (theme) => {
  const fn = async (page) => {
    if (theme === "light") await light(page);
    await sessionWithAlex(page);
    await page.waitForSelector(".cm-cwm-guest-line");
  };
  fn.clip = editorClip;
  return fn;
};

/**
 * A formatter rewrote OrderController.php on disk while Alex's edit is unsaved: the host's
 * File Cache Conflict dialog (`?cwmConflict`).
 */
const fileCacheConflict = (theme) => {
  const fn = async (page) => {
    if (theme === "light") await light(page);
    await openController(page);
    await openPopup(page);
    await page.click('.cwm-level:has-text("Edit files") input');
    await page.click('.cwm-popup button:has-text("Start Session")');
    await acceptGuest(page);
    await page.waitForSelector('.dialog:has-text("File Cache Conflict")', { timeout: 15000 });
    await page.waitForTimeout(300);
  };
  fn.query = "?cwmConflict";
  return fn;
};

/** The host chose Show Difference: the diff, and the choice kept in a sticky balloon. */
const conflictBalloon = (theme) => {
  const fn = async (page) => {
    await fileCacheConflict(theme)(page);
    await page.click('.dialog button:has-text("Show Difference")');
    await page.waitForSelector('.balloon:has-text("File Cache Conflict")');
    await page.waitForTimeout(500);
  };
  fn.query = "?cwmConflict";
  return fn;
};

/** The host's popup driven by the keyboard: Down moves through the entries. */
const popupKeyboard = (theme) => {
  const fn = async (page) => {
    if (theme === "light") await light(page);
    await sessionWithAlex(page);
    await openPopup(page);
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowDown");
    await page.waitForSelector('.cwm-popup [data-action="CodeWithMe.RegenerateLink"]:focus');
  };
  fn.clip = popupClip;
  return fn;
};

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "code-with-me-popup-keyboard": popupKeyboard("dark"),
  "code-with-me-popup-keyboard-light": popupKeyboard("light"),
  "code-with-me-guest-lines": guestLines("dark"),
  "code-with-me-guest-lines-light": guestLines("light"),
  "code-with-me-file-cache-conflict": fileCacheConflict("dark"),
  "code-with-me-file-cache-conflict-light": fileCacheConflict("light"),
  "code-with-me-conflict-diff": conflictBalloon("dark"),
  "code-with-me-conflict-diff-light": conflictBalloon("light"),
  "code-with-me-follow-everyone": everyoneDark,
  "code-with-me-follow-everyone-light": everyoneLight,
  "code-with-me-following": following,
  "code-with-me-following-light": followingLight,
  "code-with-me-tool-window": tool,
  "code-with-me-tool-window-light": toolLight,
  "code-with-me-follow-menu": followMenuDark,
  "code-with-me-follow-menu-light": followMenuLight,
  "code-with-me-session-log": log,
  "code-with-me-session-log-light": logLight,
  "code-with-me-guest-menu": guestMenuDark,
  "code-with-me-guest-menu-light": guestMenuLight,
  "code-with-me-custom-permissions": custom,
  "code-with-me-custom-permissions-light": customLight,
  "code-with-me-custom-chosen": customChosen,
  "code-with-me-start-popup": startPopup,
  "code-with-me-start-popup-light": startPopupLight,
  "code-with-me-carets": carets,
  "code-with-me-carets-light": caretsLight,
  "code-with-me-share-terminal": shareTerminalMenu("dark"),
  "code-with-me-share-terminal-light": shareTerminalMenu("light"),
  "code-with-me-guest-terminal": guestTerminalOnHost("dark"),
  "code-with-me-guest-terminal-light": guestTerminalOnHost("light"),
  "code-with-me-carets-idle": caretsIdle("dark"),
  "code-with-me-carets-idle-light": caretsIdle("light"),
  "code-with-me-session-popup": sessionPopup,
  "code-with-me-session-popup-light": sessionPopupLight,
  "code-with-me-approval": approval,
  "code-with-me-approval-light": approvalLight,
  "code-with-me-menu": menu,
  "code-with-me-menu-light": menuLight,
  "code-with-me-settings": settings,
  "code-with-me-settings-light": settingsLight,
  "code-with-me-two-pages": twoPagesDark,
  "code-with-me-two-pages-light": twoPagesLight,
};
