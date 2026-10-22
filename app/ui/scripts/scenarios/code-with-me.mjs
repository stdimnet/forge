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

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "code-with-me-start-popup": startPopup,
  "code-with-me-start-popup-light": startPopupLight,
  "code-with-me-carets": carets,
  "code-with-me-carets-light": caretsLight,
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
