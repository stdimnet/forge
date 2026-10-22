/**
 * Terminal rendering and program progress: the recorded Claude Code session from
 * `src/mock/terminalReplays.ts` (truecolor, box drawing, braille spinner, emoji, Nerd Font
 * glyphs, OSC 8 link, OSC 0 title, OSC 9;4 progress) and Settings > Tools > Terminal.
 */

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  const y = box.y + box.height / 2;
  const target = 900 - 26 - height;
  await page.mouse.move(box.x + 200, y);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, target, { steps: 4 });
  await page.mouse.up();
}

const light = (page) => page.evaluate(() => (document.documentElement.dataset.theme = "light"));

/** Open the terminal and run a recorded session; wait until the status bar shows `until`. */
async function replay(page, command, until) {
  await page.keyboard.press("Alt+F12");
  await page.waitForSelector(".term-host .xterm-screen");
  await bottomHeight(page, 560);
  await page.waitForTimeout(300);
  await page.keyboard.type(command, { delay: 8 });
  await page.keyboard.press("Enter");
  await page.waitForSelector(`[data-status="terminal-progress"]:has-text("${until}")`, { timeout: 15000 });
  await page.waitForTimeout(600);
  // Unrelated demo notifications (e.g. "Database found in .env") would cover the terminal.
  for (const close of await page.$$(".balloon-close")) await close.click();
  const renderer = await page.locator(".term-host").first().getAttribute("data-renderer");
  console.log(`  terminal renderer: ${renderer}`);
}

const session = (page) => replay(page, "claude", "60%");
const thinking = (page) => replay(page, "claude --thinking", "Working");
const failed = (page) => replay(page, "claude --error", "Error 60%");

async function settingsPage(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  const node = page.locator('.set-tree-row[data-node="tools.terminal"]');
  if (!(await node.isVisible().catch(() => false))) await page.click('.set-tree-row[data-node="tools"] .set-tree-chevron');
  await node.click();
  await page.waitForSelector('[data-setting="terminal.renderer"]');
  for (const close of await page.$$(".balloon-close")) await close.click();
  // Show the new Font and Rendering sections at the end of the page.
  await page.locator('[data-setting="terminal.scrollback"]').scrollIntoViewIfNeeded();
  await page.waitForTimeout(150);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "terminal-claude-session": session,
  "terminal-claude-session-light": async (page) => {
    await light(page);
    await session(page);
  },
  "terminal-progress-indeterminate": thinking,
  "terminal-progress-indeterminate-light": async (page) => {
    await light(page);
    await thinking(page);
  },
  "terminal-progress-error": failed,
  "terminal-progress-error-light": async (page) => {
    await light(page);
    await failed(page);
  },
  // Terminal hidden: the stripe icon and the status bar still show the progress.
  "terminal-progress-hidden": async (page) => {
    await session(page);
    await page.click('[data-tool-window="terminal"]');
    await page.waitForTimeout(200);
  },
  "terminal-settings": settingsPage,
  "terminal-settings-light": async (page) => {
    await light(page);
    await settingsPage(page);
  },
};
