/**
 * Shared debugging layer (Debug Adapter Protocol): a simulated C program under LLDB with
 * threads, variables, inline values, a logpoint, the Memory, Disassembly and Registers
 * views, and Attach to Process.
 */

/** @param {import("playwright-core").Page} page */
async function closeBalloons(page) {
  for (const close of await page.locator(".balloon-close").all()) await close.click().catch(() => {});
}

/** @param {import("playwright-core").Page} page */
async function openDemo(page) {
  await page.click('.tree-row[data-path="scripts"]');
  await page.click('.tree-row[data-path="scripts/native"]');
  await page.click('.tree-row[data-path="scripts/native/demo.c"]');
  await page.waitForFunction(() => [...document.querySelectorAll(".cm-content")].some((c) => c.textContent?.includes("compute(4)")));
  await page.waitForTimeout(300);
}

/** Click the line number `line` in the gutter (toggles a breakpoint). */
async function gutter(page, line, modifiers = []) {
  const cell = page.locator(".cm-lineNumbers:visible .cm-gutterElement").filter({ hasText: new RegExp(`^${line}$`) }).first();
  await cell.click({ modifiers });
}

/** @param {import("playwright-core").Page} page */
async function tallerBottom(page) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (box) {
    await page.mouse.move(box.x + 300, box.y + 1);
    await page.mouse.down();
    await page.mouse.move(box.x + 300, box.y - 170, { steps: 6 });
    await page.mouse.up();
  }
}

/** A breakpoint on `counter += origin.x` and a logpoint in the loop. */
async function breakpoints(page) {
  await gutter(page, 18);
  await page.waitForSelector(".cm-bp-dot");
  await gutter(page, 8, ["Shift"]);
  await page.waitForSelector(".dbg-bp-popup");
  await page.fill("#dbg-bp-log-pop", "i={i} sum={sum}");
  await page.keyboard.press("Enter");
  await page.click(".dbg-bp-popup .btn-primary");
}

/** Attach to the running "demo" process and wait for the breakpoint. */
async function attachPaused(page) {
  await closeBalloons(page);
  await openDemo(page);
  await breakpoints(page);
  await page.keyboard.press("Control+Alt+F5");
  await page.waitForSelector(".dbg-process-row[data-pid='4242']");
  await page.click(".dbg-process-row[data-pid='4242']");
  await page.click(".dbg-attach .btn-primary");
  await page.waitForSelector(".cm-exec-line");
  await page.waitForSelector('.dbg-row[data-var="origin"]');
  await tallerBottom(page);
  await page.evaluate(() => window.__forgeRunCommand("ShowExecutionPoint"));
  await page.waitForTimeout(200);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "dap-session": async (page) => {
    await attachPaused(page);
    await page.click('.dbg-row[data-var="origin"] .tree-chevron');
    await page.waitForSelector('.dbg-row[data-var="origin.x"]');
    await page.waitForSelector(".cm-debug-inline");
  },
  "dap-console-logpoint": async (page) => {
    await attachPaused(page);
    await page.click('.dbg-tab[data-tab="console"]');
    await page.waitForSelector(".dbg-out-log");
    await page.fill(".dbg-console-input input", "counter + orig");
    await page.waitForSelector(".dbg-completions, .dbg-completion", { timeout: 3000 }).catch(() => {});
  },
  "dap-breakpoints": async (page) => {
    await closeBalloons(page);
    await openDemo(page);
    await breakpoints(page);
    await page.keyboard.press("Control+Shift+F8");
    await page.waitForSelector(".dbg-breakpoints");
    await page.click('.dbg-bp-item:has-text("demo.c:8")');
    await page.waitForSelector("#dbg-bp-log-dlg");
  },
  "dap-memory": async (page) => {
    await attachPaused(page);
    await page.click('.dbg-row[data-var="counter"]', { button: "right" });
    await page.click('.run-menu-item:has-text("View Memory")');
    await page.waitForSelector(".dbg-mem-row");
  },
  "dap-disassembly": async (page) => {
    await attachPaused(page);
    await page.click('.dbg-tab[data-tab="disassembly"]');
    await page.waitForSelector(".dbg-asm-row.current");
  },
  "dap-registers": async (page) => {
    await attachPaused(page);
    await page.selectOption(".dbg-thread-select", "1");
    await page.click('.dbg-tab[data-tab="registers"]');
    await page.waitForSelector('.dbg-registers .dbg-row[data-var="$rip"]');
  },
  "dap-attach-process": async (page) => {
    await closeBalloons(page);
    await page.keyboard.press("Control+Alt+F5");
    await page.waitForSelector(".dbg-process-row[data-pid='4242']");
    await page.fill(".dbg-attach-filter", "php");
    await page.click(".dbg-process-row[data-pid='1230']");
  },
};
