/**
 * AI in the editor (browser mode: a fake provider with canned answers, cleaned by the same
 * Rust code as the desktop app): inline ghost text completion and its status bar widget,
 * "Explain with AI" / "Fix with AI" in Alt+Enter, the diff preview of a fix, Generate
 * Documentation / Tests with AI, and Settings | Tools | AI Assistant.
 */

/** @param {import("playwright-core").Page} page @param {string} name */
async function openFile(page, name) {
  await page.keyboard.press("Control+Shift+N");
  await page.waitForSelector(".se-popup");
  await page.keyboard.press("Control+A");
  await page.keyboard.type(name);
  await page.waitForTimeout(800);
  await page.keyboard.press("Enter");
  await page.waitForSelector(".se-popup", { state: "detached" });
  await page.waitForFunction((n) => document.querySelector(".editor-tab.active, [role=tab][aria-selected=true]")?.textContent?.includes(n), name, { timeout: 5000 }).catch(() => undefined);
  await page.waitForTimeout(500);
}

/**
 * Click into the `nth` occurrence of `text` in the visible editor, `offset` characters in.
 * @param {import("playwright-core").Page} page @param {string} text
 */
async function clickText(page, text, nth = 0, offset = 1) {
  const { x, y } = await page.evaluate(
    ([text, nth, offset]) => {
      const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
      if (!root) throw new Error("no editor");
      for (const line of root.querySelectorAll(".cm-line")) {
        const content = line.textContent ?? "";
        let at = content.indexOf(text);
        while (at >= 0) {
          if (nth-- === 0) {
            const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
            let rest = at + offset;
            let node = walker.nextNode();
            while (node && rest > (node.textContent ?? "").length) {
              rest -= (node.textContent ?? "").length;
              node = walker.nextNode();
            }
            const range = document.createRange();
            range.setStart(node, rest);
            range.setEnd(node, Math.min(rest + 1, (node.textContent ?? "").length));
            const r = range.getBoundingClientRect();
            return { x: r.left + 1, y: r.top + r.height / 2 };
          }
          at = content.indexOf(text, at + 1);
        }
      }
      throw new Error(`text not found: ${text}`);
    },
    [text, nth, offset],
  );
  await page.mouse.click(x, y);
  await page.mouse.move(1380, 760);
}

/** Close unrelated balloons. @param {import("playwright-core").Page} page */
async function quiet(page) {
  await page.waitForTimeout(200);
  await page.evaluate(() => document.querySelectorAll(".balloon-close").forEach((b) => b.click()));
}

/** Turn inline completion on and type a new method in PriceCalculator. @param {import("playwright-core").Page} page */
async function ghostText(page) {
  await page.evaluate(() => window.__forgeRunCommand("ai.toggleInlineCompletion"));
  await openFile(page, "PriceCalculator.php");
  await quiet(page);
  // After the last method: Ctrl+End, up to "// endregion".
  await clickText(page, "final class", 0, 2);
  await page.keyboard.press("Control+End");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Enter");
  await page.keyboard.type("public function subtotal(Order $order): float");
  await page.keyboard.press("Enter");
  await page.keyboard.type("{");
  await page.keyboard.press("Enter");
  // Indent the body like PHPStorm; typing pauses, then the suggestion appears.
  await page.keyboard.type("    ");
  await page.waitForSelector(".cm-ai-ghost", { timeout: 5000 });
  // Scrolling keeps the suggestion (only typing or moving the caret drops it).
  await page.evaluate(() => {
    const s = document.querySelector(".editor-slot:not(.hidden) .cm-scroller");
    if (s) s.scrollTop += 260;
  });
  await quiet(page);
  await page.waitForTimeout(200);
}

/** Alt+Enter on the deprecated utf8_encode() call. @param {import("playwright-core").Page} page */
async function intentions(page) {
  await openFile(page, "InvoiceService.php");
  await quiet(page);
  await page.waitForTimeout(800);
  await clickText(page, "utf8_encode", 0, 3);
  await page.keyboard.press("Alt+Enter");
  await page.waitForSelector('.intention-popup .intention-row:has-text("Fix with AI")');
  await quiet(page);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "ai-inline-completion": async (page) => {
    await ghostText(page);
  },
  "ai-inline-completion-accepted": async (page) => {
    await ghostText(page);
    await page.keyboard.press("Tab");
    await page.waitForSelector('.cm-line:has-text("return $subtotal;")');
    await page.waitForTimeout(200);
  },
  "ai-inline-status-menu": async (page) => {
    await ghostText(page);
    await page.click(".status-item.ai-status");
    await page.waitForSelector('text="Enable Inline Completion"');
    await page.waitForTimeout(200);
  },
  "ai-intentions": async (page) => {
    await intentions(page);
  },
  "ai-fix-preview": async (page) => {
    await intentions(page);
    await page.click('.intention-popup .intention-row:has-text("Fix with AI")');
    await page.waitForSelector(".ai-proposal .chat-diff-line.add");
    await quiet(page);
    await page.waitForTimeout(200);
  },
  "ai-fix-preview-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await intentions(page);
    await page.click('.intention-popup .intention-row:has-text("Fix with AI")');
    await page.waitForSelector(".ai-proposal .chat-diff-line.add");
    await quiet(page);
    await page.waitForTimeout(200);
  },
  "ai-explain-chat": async (page) => {
    await intentions(page);
    await page.click('.intention-popup .intention-row:has-text("Explain with AI")');
    await page.waitForSelector(".chat-turn-footer", { timeout: 10000 });
    await page.waitForTimeout(300);
  },
  "ai-generate-menu": async (page) => {
    await openFile(page, "InvoiceService.php");
    await quiet(page);
    await clickText(page, "settle", 0, 2);
    await page.keyboard.press("Alt+Insert");
    await page.waitForSelector('.php-generate-popup :text("Documentation with AI")');
    await page.waitForTimeout(200);
  },
  "ai-generate-docs-preview": async (page) => {
    await openFile(page, "InvoiceService.php");
    await quiet(page);
    await clickText(page, "settle", 0, 2);
    await page.evaluate(() => window.__forgeRunCommand("ai.generateDocs"));
    await page.waitForSelector(".ai-proposal .chat-diff-line.add");
    await quiet(page);
    await page.waitForTimeout(200);
  },
  "ai-generate-tests-preview": async (page) => {
    await openFile(page, "InvoiceService.php");
    await quiet(page);
    await clickText(page, "settle", 0, 2);
    await page.evaluate(() => window.__forgeRunCommand("ai.generateTests"));
    await page.waitForSelector(".ai-proposal .chat-diff-line.add");
    await quiet(page);
    await page.waitForTimeout(200);
  },
  "ai-assistant-settings": async (page) => {
    await page.evaluate(() => window.__forgeRunCommand("ai.toggleInlineCompletion"));
    await page.evaluate(() => window.__forgeRunCommand("ai.assistantSettings"));
    await page.waitForSelector(".ai-assistant-model select");
    await quiet(page);
    await page.waitForTimeout(300);
  },
};
