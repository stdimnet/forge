/** AI Chat tool window: empty state, a turn mid-stream, and a finished turn with a diff. */

async function openChat(page) {
  await page.click('[data-tool-window="ai-chat"]');
  await page.waitForSelector(".chat-panel");
}

async function ask(page, text) {
  // These scenarios show a session without prompts: allow every tool call right away
  // (permission cards have their own scenarios in integration-polish.mjs).
  await page.evaluate(() => {
    setInterval(() => document.querySelector(".chat-permission-card .btn-primary")?.click(), 50);
  });
  await page.fill(".chat-input", text);
  await page.keyboard.press("Enter");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "ai-chat-empty": async (page) => {
    await openChat(page);
    await page.waitForSelector(".chat-empty-status");
  },
  "ai-chat-streaming": async (page) => {
    await page.click('.tree-row[data-path="app/Models/Order.php"]');
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector('.chat-tool-card[data-tool="Bash"].running', { timeout: 30000 });
    await page.waitForTimeout(400);
  },
  "ai-chat-finished": async (page) => {
    await page.click('.tree-row[data-path="app/Models/Order.php"]');
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-turn-footer", { timeout: 40000 });
    await page.click('.chat-tool-card[data-tool="MultiEdit"] .chat-tool-verb');
    await page.evaluate(() => {
      document.querySelector('.chat-tool-card[data-tool="MultiEdit"]')?.scrollIntoView({ block: "start" });
    });
  },
  "ai-chat-diff-dialog": async (page) => {
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-turn-footer", { timeout: 40000 });
    await page.click('.chat-tool-card[data-tool="Edit"] .chat-tool-verb');
    await page.click('.chat-tool-card[data-tool="Edit"] .chat-tool-actions button');
    await page.waitForSelector(".chat-dialog");
  },
  "ai-chat-agent-picker": async (page) => {
    await openChat(page);
    await page.click(".chat-toolbar .chat-dropdown:first-child .chat-dropdown-trigger");
    await page.waitForSelector(".chat-dropdown-menu");
  },
  "ai-chat-attach": async (page) => {
    await openChat(page);
    await page.click(".chat-input");
    await page.keyboard.type("Explain this @");
    await page.waitForSelector(".chat-attach-menu");
  },
  "ai-chat-interrupted": async (page) => {
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector('.chat-tool-card[data-tool="Read"]', { timeout: 30000 });
    await page.click(".chat-send.stop");
    await page.waitForSelector(".chat-turn-note");
  },
  "ai-chat-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.click('.tree-row[data-path="app/Models/Order.php"]');
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-turn-footer", { timeout: 40000 });
  },
};
