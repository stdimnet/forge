/**
 * Keyboard-only flow through browser mode, without a single mouse event: open a file (Go to
 * File), rename a file from the Project view (Shift+F6), focus cycling between the areas,
 * the Commit tool window with its message and Commit, and the Settings dialog (tree keys, a
 * checkbox, OK). Every step checks where focus is and that it is visible.
 *
 * Usage: pnpm dev &  then  pnpm a11y:keyboard   (exit code 1 on the first failed step)
 * FORGE_URL / CHROMIUM_PATH override the defaults like in scripts/screenshots.mjs.
 */
import { chromium } from "playwright-core";
import { existsSync } from "node:fs";

const url = process.env.FORGE_URL ?? "http://localhost:1420";
const executablePath =
  process.env.CHROMIUM_PATH ?? ["/opt/pw-browsers/chromium-1194/chrome-linux/chrome"].find((p) => existsSync(p));

const browser = await chromium.launch({ executablePath });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
// Any mouse event fails the run: the flow must work with the keyboard alone.
await page.addInitScript(() => {
  // Enter and Space on a button fire a click too, with detail 0; a pointer click has detail ≥ 1.
  for (const type of ["mousedown", "click", "pointerdown"])
    window.addEventListener(type, (e) => e.isTrusted && e.detail > 0 && (window.__forgeMouseUsed = type), true);
});
await page.goto(url);
await page.waitForSelector(".cm-content", { timeout: 90000 });
await page.waitForTimeout(600);

let step = 0;
async function check(what, fn) {
  step++;
  try {
    await fn();
    console.log(`ok ${step} - ${what}`);
  } catch (e) {
    console.log(`not ok ${step} - ${what}\n  ${e.message.split("\n")[0]}`);
    await page.screenshot({ path: "a11y-keyboard-failure.png" });
    await browser.close();
    process.exit(1);
  }
}

const press = (keys) => page.keyboard.press(keys);
const focused = () =>
  page.evaluate(() => {
    const el = document.activeElement;
    const ring = el ? getComputedStyle(el).outlineStyle : "none";
    return {
      tag: el?.tagName.toLowerCase(),
      role: el?.getAttribute("role"),
      cls: el?.className?.toString() ?? "",
      label: el?.getAttribute("aria-label") ?? "",
      inDialog: !!el?.closest("[role=dialog]"),
      area: el?.closest(".main-toolbar, .tool-stripe, .tool-window, .editor-area, .status-bar")?.className.split(" ")[0] ?? "",
      ring,
    };
  });
const expect = (cond, message) => {
  if (!cond) throw new Error(message);
};

await check("Go to File opens a file", async () => {
  await press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.type("OrderController");
  await page.waitForTimeout(400);
  const f = await focused();
  expect(f.role === "combobox", `search field is not a combobox: ${JSON.stringify(f)}`);
  await press("Enter");
  await page.waitForSelector('.editor-tab.active[data-tab-id$="OrderController.php"]', { timeout: 5000 });
  await page.waitForTimeout(300);
  expect((await focused()).cls.includes("cm-content"), "the editor did not get focus");
});

await check("Alt+1 focuses the Project tree, arrows move the selection", async () => {
  if (!(await page.$(".project-tree"))) await press("Alt+1");
  else {
    await press("Alt+1"); // hide
    await press("Alt+1"); // show and focus
  }
  await page.waitForTimeout(300);
  const f = await focused();
  expect(f.role === "tree", `focus is not on the tree: ${JSON.stringify(f)}`);
  await press("Home");
  await press("ArrowDown");
  const active = await page.evaluate(() => document.activeElement?.getAttribute("aria-activedescendant"));
  expect(!!active, "the tree has no active row");
});

await check("Shift+F6 renames a file from the Project view", async () => {
  await page.evaluate(async () => {
    const tree = document.querySelector(".project-tree");
    const rows = [...tree.querySelectorAll(".tree-row[data-path]")];
    // Select composer.json with the keyboard: walk down to it.
    const index = rows.findIndex((r) => r.dataset.path === "composer.json");
    window.__forgeRowIndex = index;
  });
  const index = await page.evaluate(() => window.__forgeRowIndex);
  await press("Home");
  for (let i = 0; i < index; i++) await press("ArrowDown");
  await press("Shift+F6");
  await page.waitForSelector("[role=dialog] input", { timeout: 5000 });
  const f = await focused();
  expect(f.inDialog && f.tag === "input", `the rename field has no focus: ${JSON.stringify(f)}`);
  const labelled = await page.evaluate(() => document.activeElement.labels?.length || document.activeElement.getAttribute("aria-label"));
  expect(!!labelled, "the rename field has no label");
  await page.keyboard.type("composer-renamed");
  await press("Tab");
  expect((await focused()).inDialog, "Tab left the dialog");
  await press("Escape");
  await page.waitForTimeout(200);
  expect(!(await page.$("[role=dialog] input")), "Escape did not close the dialog");
  const back = await focused();
  expect(back.role === "tree", `focus did not return to the tree: ${JSON.stringify(back)}`);
});

await check("Escape in a tool window returns to the editor", async () => {
  await press("Escape");
  await page.waitForTimeout(150);
  expect((await focused()).cls.includes("cm-content"), "Escape did not focus the editor");
});

await check("Ctrl+Shift+F6 cycles focus between areas with a visible focus ring", async () => {
  const seen = [];
  for (let i = 0; i < 6; i++) {
    await press("Control+Shift+F6");
    await page.waitForTimeout(80);
    const f = await focused();
    seen.push(f.area);
    if (f.tag === "button") expect(f.ring !== "none", `no focus ring on ${JSON.stringify(f)}`);
  }
  expect(new Set(seen).size >= 3, `focus visited too few areas: ${seen.join(", ")}`);
  await press("Control+Alt+Shift+F6");
});

await check("Ctrl+Tab opens the Switcher", async () => {
  await page.keyboard.down("Control");
  await press("Tab");
  await page.waitForSelector(".recent-popup");
  await page.keyboard.up("Control");
  await page.waitForTimeout(200);
  expect(!(await page.$(".recent-popup")), "the Switcher did not close on releasing Ctrl");
});

await check("Ctrl+K opens the Commit tool window; the changes tree works with the keyboard", async () => {
  await press("Control+k");
  await page.waitForSelector(".commit-row", { timeout: 5000 });
  await page.waitForTimeout(300);
  const f = await focused();
  expect(f.inDialog || f.area === "tool-window" || f.tag === "textarea" || f.cls.includes("cm-content"), `focus not in the Commit window: ${JSON.stringify(f)}`);
  await page.focus(".commit-tree");
  await press("Home");
  await press("ArrowDown");
  const active = await page.evaluate(() => document.querySelector(".commit-tree").getAttribute("aria-activedescendant"));
  expect(!!active, "the changes tree has no active row");
  const before = await page.evaluate(() => document.querySelector(".commit-row .commit-check").checked);
  await press(" ");
  const after = await page.evaluate(() => document.querySelector(".commit-row .commit-check").checked);
  expect(before !== after, "Space did not toggle the file");
  await press(" ");
  // Tab from the tree reaches the commit message; Ctrl+Enter commits.
  for (let i = 0; i < 10 && (await focused()).tag !== "textarea"; i++) await press("Tab");
  expect((await focused()).tag === "textarea", "Tab did not reach the commit message");
  await page.keyboard.type("Update the order flow");
  const rows = await page.evaluate(() => document.querySelectorAll(".commit-row").length);
  await press("Control+Enter");
  await page.waitForFunction((n) => document.querySelectorAll(".commit-row").length < n, rows, { timeout: 5000 });
});

await check("Shift+Escape hides the active tool window", async () => {
  await page.focus(".commit-tree");
  await press("Shift+Escape");
  await page.waitForTimeout(150);
  expect(!(await page.$('[data-tool-window-panel="commit"]')), "the Commit window is still shown");
});

await check("Settings: search, tree keys, a checkbox and OK", async () => {
  await press("Control+Alt+s");
  await page.waitForSelector(".set-tree-row");
  await page.waitForTimeout(300);
  let f = await focused();
  expect(f.inDialog && f.tag === "input", `the search field has no focus: ${JSON.stringify(f)}`);
  await page.keyboard.type("screen reader");
  await page.waitForTimeout(400);
  await press("Escape"); // clears the search
  await page.waitForTimeout(200);
  // Tab into the tree; arrows select pages.
  await press("Tab");
  f = await focused();
  expect(f.role === "treeitem", `Tab did not reach the tree: ${JSON.stringify(f)}`);
  expect(f.ring !== "none", "the tree row shows no focus ring");
  await press("Home");
  await press("ArrowDown");
  f = await focused();
  expect(f.role === "treeitem", "ArrowDown left the tree");
  // Appearance & Behavior | Appearance: the first child of the first group.
  await press("Home");
  if (!(await page.evaluate(() => document.activeElement.getAttribute("aria-expanded") === "true"))) await press("ArrowRight");
  await press("ArrowDown");
  f = await focused();
  expect((await page.evaluate(() => document.activeElement.dataset.node)) === "appearance", "Appearance is not selected");
  await press("Enter"); // into the page
  f = await focused();
  expect(f.inDialog && f.role !== "treeitem", `Enter did not move into the page: ${JSON.stringify(f)}`);
  // Tab through the page to the screen reader checkbox and turn it on.
  for (let i = 0; i < 40; i++) {
    const id = await page.evaluate(() => document.activeElement?.id);
    if (id === "setting-appearance-screenReader") break;
    await press("Tab");
    expect((await focused()).inDialog, "Tab left the Settings dialog");
  }
  expect((await page.evaluate(() => document.activeElement?.id)) === "setting-appearance-screenReader", "the checkbox was not reached");
  const described = await page.evaluate(() => document.activeElement.getAttribute("aria-describedby"));
  expect(!!described, "the checkbox has no description");
  await press(" ");
  // OK is reached with Tab as well; Enter on it applies and closes.
  for (let i = 0; i < 80; i++) {
    const text = await page.evaluate(() => document.activeElement?.textContent?.trim());
    if (text === "OK") break;
    await press("Tab");
  }
  await press("Enter");
  await page.waitForTimeout(400);
  expect(!(await page.$(".set-tree-row")), "the Settings dialog did not close");
  const on = await page.evaluate(() => document.documentElement.dataset.screenReader !== undefined);
  expect(on, "the setting was not applied");
});

await check("Alt+Shift+L announces the caret position", async () => {
  await press("Escape");
  await page.evaluate(() => document.querySelector(".editor-slot:not(.hidden) .cm-content")?.focus());
  await press("Alt+Shift+L");
  await page.waitForTimeout(150);
  const said = await page.evaluate(() => document.querySelector("[data-live-region=assertive]")?.textContent ?? "");
  expect(/Line \d+, column \d+/.test(said), `nothing announced: "${said}"`);
});

await check("Settings | Keymap: the shortcut field has a name and Shift+Tab leaves it (#217)", async () => {
  await press("Control+Alt+s");
  await page.waitForSelector(".set-tree-row");
  await page.waitForTimeout(300);
  // Tab into the tree, then walk down to Keymap.
  await press("Tab");
  await press("Home");
  for (let i = 0; i < 40 && (await page.evaluate(() => document.activeElement?.dataset.node)) !== "keymap"; i++) await press("ArrowDown");
  expect((await page.evaluate(() => document.activeElement?.dataset.node)) === "keymap", "Keymap was not reached in the tree");
  await press("Enter");
  await page.waitForSelector(".keymap-page");
  // Tab to "Find Actions by Shortcut" and open it.
  for (let i = 0; i < 40; i++) {
    if ((await page.evaluate(() => document.activeElement?.getAttribute("title"))) === "Find Actions by Shortcut") break;
    await press("Tab");
  }
  await press("Enter");
  await page.waitForSelector(".km-find-popup .shortcut-field");
  await page.waitForTimeout(100);
  let f = await focused();
  expect(f.role === "textbox" && f.label === "Find Actions by Shortcut", `the shortcut field has no role or name: ${JSON.stringify(f)}`);
  await press("Shift+Tab");
  f = await focused();
  expect(!f.cls.includes("shortcut-field"), "Shift+Tab did not leave the shortcut field");
  const recorded = await page.evaluate(() => document.querySelector(".km-find-popup .shortcut-field")?.classList.contains("empty") ?? true);
  expect(recorded, "Shift+Tab was recorded as a shortcut");
  await press("Escape");
  await page.waitForTimeout(150);
  if (await page.$(".set-tree-row")) await press("Escape");
  await page.waitForTimeout(200);
  expect(!(await page.$(".set-tree-row")), "the Settings dialog did not close");
});

await check("Diff viewer: focus inside, F7 through the keymap, Escape closes it (#217)", async () => {
  await press("Control+k");
  await page.waitForSelector(".commit-tree", { timeout: 5000 });
  await page.waitForTimeout(300);
  if (!(await page.$(".commit-row"))) return; // everything was committed above
  await page.focus(".commit-tree");
  await press("Home");
  // Walk down to the first file (changelist rows have no diff).
  const onFile = () =>
    page.evaluate(() => {
      const id = document.querySelector(".commit-tree")?.getAttribute("aria-activedescendant");
      return !!(id && document.getElementById(id)?.dataset.path);
    });
  for (let i = 0; i < 6 && !(await onFile()); i++) await press("ArrowDown");
  expect(await onFile(), "no file row in the Commit window");
  await press("Control+d");
  await page.waitForSelector(".diff-viewer", { timeout: 5000 });
  await page.waitForTimeout(400);
  let f = await focused();
  expect(await page.evaluate(() => !!document.activeElement?.closest(".diff-viewer")), `the diff viewer has no focus: ${JSON.stringify(f)}`);
  await press("F7");
  await page.waitForTimeout(150);
  expect(!!(await page.$(".diff-viewer")), "F7 closed the diff viewer");
  await press("Escape");
  await page.waitForTimeout(300);
  expect(!(await page.$(".diff-viewer")), "Escape did not close the diff viewer");
  await press("Shift+Escape");
});

await check("Generate chooser: Escape closes it once and the editor gets the focus back (#217)", async () => {
  await press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.type("User.php");
  await page.waitForTimeout(400);
  await press("Enter");
  await page.waitForSelector('.editor-tab.active[data-tab-id$="User.php"]', { timeout: 5000 });
  await page.evaluate(async () => {
    const { activeView } = await import("/src/editor/views.ts");
    const { editor } = await import("/src/state/editor.ts");
    const { startGenerate } = await import("/src/features/php/GenerateOverlay.tsx");
    const view = activeView();
    const at = view.state.doc.toString().indexOf("protected $hidden");
    view.dispatch({ selection: { anchor: at } });
    await startGenerate(editor.active, view, "getters");
  });
  await page.waitForSelector(".php-dialog[role=dialog]", { timeout: 5000 });
  await page.waitForTimeout(150);
  expect(await page.evaluate(() => !!document.activeElement?.closest(".php-dialog")), "the chooser has no focus");
  await press("Escape");
  await page.waitForTimeout(200);
  expect(!(await page.$(".php-dialog[role=dialog]")), "Escape did not close the chooser");
  expect((await focused()).cls.includes("cm-content"), "the editor did not get the focus back");
});

await check("no mouse event was used", async () => {
  const used = await page.evaluate(() => window.__forgeMouseUsed);
  expect(!used, `a ${used} event happened`);
});

await browser.close();
console.log(`\n${step} steps passed`);
