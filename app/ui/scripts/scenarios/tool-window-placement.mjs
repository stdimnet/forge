/** Tool window placement: Move To, Remove from Sidebar, split areas, ⋯ list, drag and drop. */

const button = (id) => `[data-tool-window="${id}"]`;

async function openMoveTo(page, id) {
  await page.click(button(id), { button: "right" });
  await page.hover('.context-menu-item:has-text("Move To")');
  await page.getByRole("menuitem", { name: "Right Bottom", exact: true }).waitFor();
}

async function moveTo(page, id, target) {
  await openMoveTo(page, id);
  await page.getByRole("menuitem", { name: target, exact: true }).click();
}

async function assert(page, selector, message) {
  if (!(await page.locator(selector).count())) throw new Error(message);
}

/** Drag a stripe button with the mouse onto another button (before it) and drop it. */
async function drag(page, id, onto, { drop = true } = {}) {
  const from = await page.locator(button(id)).boundingBox();
  const to = await page.locator(onto).boundingBox();
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + from.width / 2 + 8, from.y + from.height / 2 - 8, { steps: 3 });
  await page.mouse.move(to.x + to.width / 2, to.y + 4, { steps: 12 });
  if (drop) await page.mouse.up();
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "tool-window-move-to": async (page) => {
    await openMoveTo(page, "terminal");
  },

  "tool-window-moved-right": async (page) => {
    await page.click(button("terminal"));
    await page.waitForSelector('[data-tool-window-panel="terminal"]');
    await moveTo(page, "terminal", "Right Top");
    await assert(page, `.tool-stripe-right ${button("terminal")}`, "Terminal is not on the right stripe");
    await assert(page, '.tool-area-right [data-tool-window-panel="terminal"]', "Terminal is not open on the right");
    // The header's ⋮ menu has the same entries plus Hide.
    await page.click('[data-tool-window-panel="terminal"] [data-tool-window-options]');
    await page.waitForSelector('.context-menu-item:has-text("Remove from Sidebar")');
  },

  "tool-window-split-areas": async (page) => {
    await moveTo(page, "terminal", "Bottom Right");
    await page.click(button("problems"));
    await page.click(button("terminal"));
    await moveTo(page, "bookmarks", "Left Bottom");
    await page.click(button("bookmarks"));
    await page.waitForSelector('.tool-area-bottom.split [data-tool-window-panel="terminal"]');
    await assert(page, '.tool-area-left.split [data-tool-window-panel="bookmarks"]', "Left area is not split");
  },

  "tool-window-more-list": async (page) => {
    await page.click(button("problems"), { button: "right" });
    await page.click('.context-menu-item:has-text("Remove from Sidebar")');
    if (await page.locator(button("problems")).count()) throw new Error("Problems is still on the sidebar");
    await page.click("[data-more-tool-windows]");
    await page.waitForSelector('.more-tool-windows .context-menu-item:has-text("not on sidebar")');
  },

  "tool-window-drag": async (page) => {
    // Reorder inside the left stripe: Commit in front of Project.
    await drag(page, "commit", button("project"));
    const order = await page.$$eval('.tool-stripe-left [data-stripe-slot="left"] [data-tool-window]', (els) =>
      els.map((e) => e.dataset.toolWindow),
    );
    if (order[0] !== "commit" || order[1] !== "project") throw new Error(`reorder failed: ${order}`);
    // Move Terminal to the top of the right stripe.
    await drag(page, "terminal", `.tool-stripe-right ${button("ai-chat")}`);
    const right = await page.$$eval('.tool-stripe-right [data-stripe-slot="right"] [data-tool-window]', (els) =>
      els.map((e) => e.dataset.toolWindow),
    );
    if (right[0] !== "terminal") throw new Error(`drop failed: ${right}`);
    // The screenshot shows a drag in progress with its drop indicator.
    await drag(page, "git", `.tool-stripe-right ${button("database")}`, { drop: false });
    await page.waitForSelector('.tool-stripe-right .stripe-group.drop-target .stripe-drop-indicator');
  },

  "tool-window-split-areas-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await moveTo(page, "terminal", "Bottom Right");
    await page.click(button("problems"));
    await page.click(button("terminal"));
    await moveTo(page, "bookmarks", "Left Bottom");
    await page.click(button("bookmarks"));
    await page.click(button("database"), { button: "right" });
    await page.hover('.context-menu-item:has-text("Move To")');
    await page.getByRole("menuitem", { name: "Left Top", exact: true }).waitFor();
  },
};
