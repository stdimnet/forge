/** Gemini CLI in the AI Chat: agent picker, a conversation with tool calls, not installed, settings. */

async function openChat(page) {
  await page.click('[data-tool-window="ai-chat"]');
  await page.waitForSelector(".chat-panel");
}

const toolbarDropdown = (n) => `.chat-toolbar .chat-dropdown:nth-child(${n}) .chat-dropdown-trigger`;

async function pickGemini(page) {
  await page.click(toolbarDropdown(1));
  await page.click('.chat-dropdown-menu .chat-popup-item:has-text("Gemini CLI")');
  await page.waitForSelector('.chat-empty-title:has-text("Gemini CLI")');
}

/** Choose a permission mode by its label in the toolbar's mode picker. */
async function pickMode(page, label) {
  await page.click(".chat-mode .chat-dropdown-trigger");
  await page.click(`.chat-dropdown-menu .chat-popup-item:has-text("${label}")`);
}

async function ask(page, text) {
  await page.fill(".chat-input", text);
  await page.keyboard.press("Enter");
}

async function openSettingsPage(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForTimeout(150);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "gemini-agent-picker": async (page) => {
    await openChat(page);
    await page.click(toolbarDropdown(1));
    await page.waitForSelector('.chat-dropdown-menu .chat-popup-item:has-text("Gemini CLI")');
  },
  "gemini-empty": async (page) => {
    await openChat(page);
    await pickGemini(page);
    await page.waitForSelector(".chat-empty-status:not(.missing)");
  },
  "gemini-modes": async (page) => {
    await openChat(page);
    await pickGemini(page);
    await page.click(".chat-mode .chat-dropdown-trigger");
    await page.waitForSelector('.chat-dropdown-menu .chat-popup-item:has-text("YOLO")');
  },
  "gemini-conversation": async (page) => {
    await page.click('.tree-row[data-path="app/Models/Order.php"]');
    await openChat(page);
    await pickGemini(page);
    await pickMode(page, "YOLO");
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-turn-footer", { timeout: 40000 });
    await page.click('.chat-tool-card[data-tool="Edit"] .chat-tool-verb');
    await page.evaluate(() => {
      document.querySelector('.chat-tool-card[data-tool="Read"]')?.scrollIntoView({ block: "start" });
    });
  },
  "gemini-conversation-end": async (page) => {
    await openChat(page);
    await pickGemini(page);
    await pickMode(page, "YOLO");
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-turn-footer", { timeout: 40000 });
    await page.click('.chat-tool-card[data-tool="Bash"] .chat-tool-verb, .chat-tool-card[data-tool="Bash"] .chat-tool-header');
  },
  "gemini-read-only": async (page) => {
    await openChat(page);
    await pickGemini(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-turn-footer", { timeout: 40000 });
  },
  "gemini-not-installed": async (page) => {
    await openSettingsPage(page, "tools.ai-agents");
    await page.fill('[data-setting="agents.gemini-cli.path"] input', "/opt/tools/gemini-old");
    await page.waitForSelector('[data-agent="gemini-cli"] .ai-agent-missing');
    await page.click(".set-footer .btn-primary");
    await page.waitForSelector(".set-dialog", { state: "detached" });
    await openChat(page);
    await pickGemini(page);
    await page.waitForSelector(".chat-empty-status.missing");
  },
  "gemini-settings": async (page) => {
    await openSettingsPage(page, "tools.ai-agents");
    await page.waitForSelector('[data-agent="gemini-cli"] .ai-agent-found');
    await page.evaluate(() => document.querySelector('[data-agent="gemini-cli"]')?.scrollIntoView({ block: "start" }));
  },
  "gemini-light": async (page) => {
    await openChat(page);
    await pickGemini(page);
    await pickMode(page, "Auto-accept edits");
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-turn-footer", { timeout: 40000 });
    // Set last: the theme setting is applied once settings have loaded.
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await page.waitForTimeout(200);
  },
};
