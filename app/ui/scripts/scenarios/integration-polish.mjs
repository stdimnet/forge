/** Milestone 1 integration: chat permission prompts, Settings > Tools > AI Agents, one PHP hover. */

/** @param {import("playwright-core").Page} page */
async function openChat(page) {
  await page.click('[data-tool-window="ai-chat"]');
  await page.waitForSelector(".chat-panel");
}

/** @param {import("playwright-core").Page} page @param {string} text */
async function ask(page, text) {
  await page.fill(".chat-input", text);
  await page.keyboard.press("Enter");
}

/** @param {import("playwright-core").Page} page @param {string} id */
async function openSettingsPage(page, id) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click(`.set-tree-row[data-node="${id}"]`);
  await page.waitForTimeout(150);
}

/** Move the mouse over `word` on the first line of the active editor containing `lineText`. */
async function hoverWord(page, lineText, word) {
  const p = await page.evaluate(
    ({ lineText, word }) => {
      for (const line of document.querySelectorAll(".editor-slot:not(.hidden) .cm-line")) {
        const text = line.textContent ?? "";
        if (!text.includes(lineText)) continue;
        let remaining = text.indexOf(word) + Math.floor(word.length / 2);
        const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          if (remaining < n.length) {
            const r = document.createRange();
            r.setStart(n, remaining);
            r.setEnd(n, remaining + 1);
            const b = r.getBoundingClientRect();
            return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
          }
          remaining -= n.length;
        }
      }
      return null;
    },
    { lineText, word },
  );
  if (!p) throw new Error(`"${word}" not found`);
  await page.mouse.move(p.x, p.y);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "chat-permission-edit": async (page) => {
    await page.click('[data-path="app/Models/Order.php"]');
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-permission-card", { timeout: 30000 });
    await page.evaluate(() => document.querySelector(".chat-permission-card")?.scrollIntoView({ block: "end" }));
    await page.waitForTimeout(200);
  },
  "chat-permission-command": async (page) => {
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    // Allow the edits (once / for the session), then stop at the test run.
    await page.click(".chat-permission-card .btn-primary", { timeout: 30000 });
    await page.waitForSelector('.chat-permission-card [aria-label]', { state: "detached" }).catch(() => {});
    await page.waitForSelector('.chat-tool-card[data-tool="MultiEdit"]', { timeout: 30000 });
    await page.locator(".chat-permission-card .btn-secondary", { hasText: "Allow for this session" }).click({ timeout: 30000 });
    await page.waitForFunction(() => document.querySelector(".chat-permission-card")?.textContent?.includes("run this command"), null, {
      timeout: 30000,
    });
    await page.evaluate(() => document.querySelector(".chat-permission-card")?.scrollIntoView({ block: "end" }));
    await page.waitForTimeout(200);
  },
  "chat-permission-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.waitForSelector(".chat-permission-card", { timeout: 30000 });
    await page.evaluate(() => document.querySelector(".chat-permission-card")?.scrollIntoView({ block: "end" }));
    await page.waitForTimeout(200);
  },
  "chat-permission-denied": async (page) => {
    await openChat(page);
    await ask(page, "Add a scope for paid orders to Order model");
    await page.locator(".chat-permission-card .btn-secondary", { hasText: "Deny" }).click({ timeout: 30000 });
    await page.waitForSelector(".chat-turn-footer", { timeout: 30000 });
  },
  "settings-ai-agents": async (page) => {
    await openSettingsPage(page, "tools.ai-agents");
    await page.waitForSelector(".ai-agent-found");
  },
  "settings-ai-agents-missing": async (page) => {
    await openSettingsPage(page, "tools.ai-agents");
    await page.fill('[data-setting="agents.claude-code.path"] input', "/opt/tools/claude-old");
    await page.waitForSelector('[data-agent="claude-code"] .ai-agent-missing');
  },
  "php-hover-merged": async (page) => {
    // `hasMany` has both a language server and an index hover: only the server's shows.
    await page.waitForSelector('.lsp-status[data-state="ready"]', { timeout: 5000 });
    await hoverWord(page, "hasMany", "hasMany");
    await page.waitForSelector(".cm-tooltip-hover .lsp-hover");
    await page.waitForTimeout(600);
    const count = await page.locator(".cm-tooltip-hover").count();
    if (count !== 1) throw new Error(`expected one hover tooltip, got ${count}`);
  },
  "php-hover-index-fallback": async (page) => {
    // No language server answer here, so the index hover fills in.
    await page.dblclick('[data-path="app/Http"]');
    await page.dblclick('[data-path="app/Http/Controllers"]');
    await page.dblclick('[data-path="app/Http/Controllers/OrderController.php"]');
    await page.waitForFunction(() => document.querySelector(".editor-tab.active")?.textContent?.includes("OrderController.php"));
    await page.waitForTimeout(300);
    await hoverWord(page, "public function show", "show");
    await page.waitForSelector(".cm-tooltip-hover .php-hover", { timeout: 3000 });
    await page.waitForTimeout(600);
    const count = await page.locator(".cm-tooltip-hover").count();
    if (count !== 1) throw new Error(`expected one hover tooltip, got ${count}`);
  },
};
