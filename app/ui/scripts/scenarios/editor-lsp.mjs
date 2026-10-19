/** Screenshots of the language server features (hover, completion, problems, parameter info). */

/** Center of the `nth` occurrence of `text` inside the active editor. */
async function textPoint(page, text, nth = 0, offset = 0.5) {
  return page.evaluate(
    ({ text, nth, offset }) => {
      const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      let seen = 0;
      // CodeMirror splits lines into highlighted spans, so search line by line.
      for (const line of root.querySelectorAll(".cm-line")) {
        const idx = line.textContent.indexOf(text);
        if (idx < 0) continue;
        if (seen++ < nth) continue;
        let remaining = idx + Math.floor(text.length * offset);
        const lw = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
        for (let n = lw.nextNode(); n; n = lw.nextNode()) {
          if (remaining <= n.length) {
            const r = document.createRange();
            r.setStart(n, Math.min(remaining, n.length - 1));
            r.setEnd(n, Math.min(remaining, n.length - 1) + 1);
            const b = r.getBoundingClientRect();
            return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
          }
          remaining -= n.length;
        }
      }
      void walker;
      return null;
    },
    { text, nth, offset },
  );
}

async function waitForServer(page) {
  await page.waitForSelector('.lsp-status[data-state="ready"]', { timeout: 5000 });
}

async function openFile(page, dir, file) {
  await page.click(`[data-path="${dir}"]`);
  await page.click(`[data-path="${dir}/${file}"]`);
  await page.waitForFunction((name) => document.querySelector(".editor-tab.active")?.textContent?.includes(name), file);
  await page.waitForTimeout(150);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "lsp-hover": async (page) => {
    await waitForServer(page);
    const p = await textPoint(page, "hasMany");
    await page.mouse.move(p.x, p.y);
    await page.waitForSelector(".cm-tooltip-hover .lsp-hover");
  },
  "lsp-hover-array-map": async (page) => {
    await waitForServer(page);
    await openFile(page, "app/Services", "InvoiceService.php");
    const p = await textPoint(page, "array_map");
    await page.mouse.move(p.x, p.y);
    await page.waitForSelector(".cm-tooltip-hover .lsp-hover");
  },
  "lsp-completion": async (page) => {
    await waitForServer(page);
    const p = await textPoint(page, "return $this->hasMany(Order::class);", 0, 1);
    await page.mouse.click(p.x + 20, p.y);
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await page.keyboard.type("$this->", { delay: 30 });
    await page.waitForSelector(".cm-tooltip-autocomplete");
    await page.waitForTimeout(400);
  },
  "lsp-parameter-info": async (page) => {
    await waitForServer(page);
    const p = await textPoint(page, "Order::class);", 0, 0.9);
    await page.mouse.click(p.x, p.y);
    await page.keyboard.press("Control+p");
    await page.waitForSelector(".cm-lsp-signature");
  },
  "lsp-problems": async (page) => {
    await waitForServer(page);
    await openFile(page, "app/Services", "InvoiceService.php");
    await page.click('[data-tool-window="problems"]');
    await page.waitForSelector(".problem-row");
    await page.mouse.move(700, 120);
  },
  "lsp-project-errors": async (page) => {
    await waitForServer(page);
    await openFile(page, "app/Services", "InvoiceService.php");
    await page.click('[data-tool-window="problems"]');
    await page.click(".problems-tab:nth-child(2)");
    await page.waitForSelector(".problem-row");
    const p = await textPoint(page, "markAsPaid");
    await page.mouse.move(p.x, p.y);
    await page.waitForSelector(".cm-tooltip-hover .lsp-hover");
  },
};
