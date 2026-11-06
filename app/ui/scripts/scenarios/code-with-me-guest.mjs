/**
 * Code With Me, guest side (S3): the host's project in a guest's window, with search and
 * language features answered by the host. Browser mode simulates the host with the demo
 * project (`?project=cwm-guest`); dark (Ember) and light (Anvil).
 */

const light = (page) => page.evaluate(() => (document.documentElement.dataset.theme = "light"));

/**
 * Center of the first occurrence of `text` inside the active editor, scrolled into view first
 * (rendered doc comments can push it below the editor's bottom edge).
 */
async function textPoint(page, text) {
  await page.evaluate((text) => {
    const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
    const line = [...root.querySelectorAll(".cm-line")].find((l) => l.textContent.includes(text));
    line?.scrollIntoView({ block: "center" });
  }, text);
  // Inlays and rendered doc comments settle after scrolling: wait until the text stops moving.
  let last = null;
  for (let i = 0; i < 20; i++) {
    await page.waitForTimeout(150);
    const p = await locate(page, text);
    if (p && last && p.x === last.x && p.y === last.y) return p;
    last = p;
  }
  return last;
}

function locate(page, text) {
  return page.evaluate((text) => {
    const root = document.querySelector(".editor-slot:not(.hidden) .cm-content");
    for (const line of root.querySelectorAll(".cm-line")) {
      const idx = line.textContent.indexOf(text);
      if (idx < 0) continue;
      let remaining = idx + Math.floor(text.length / 2);
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
    return null;
  }, text);
}

const project = (theme) => async (page) => {
  await page.waitForSelector("[data-testid=cwm-status]");
  await page.dblclick('.tree-row[data-path="app"]');
  await page.dblclick('.tree-row[data-path="app/Models"]');
  if (theme === "light") await light(page);
};
const search = (theme) => async (page) => {
  await page.waitForSelector("[data-testid=cwm-status]");
  if (theme === "light") await light(page);
  await page.click(".cm-content");
  await page.keyboard.press("Shift");
  await page.keyboard.press("Shift");
  await page.waitForSelector(".se-popup");
  await page.keyboard.type("order");
  await page.waitForTimeout(300);
  await page.keyboard.press("ArrowDown");
};
const hover = (theme) => async (page) => {
  await page.waitForSelector("[data-testid=cwm-status]");
  await page.waitForSelector('.lsp-status[data-state="ready"]', { timeout: 5000 });
  if (theme === "light") await light(page);
  const p = await textPoint(page, "hasMany");
  await page.mouse.move(p.x, p.y);
  await page.waitForSelector(".cm-tooltip-hover .lsp-hover");
};
const status = (theme) => async (page) => {
  await page.waitForSelector("[data-testid=cwm-status]");
  await page.waitForFunction(() => document.querySelector("[data-testid=cwm-status] .cwm-status-detail"));
  if (theme === "light") await light(page);
  await page.click("[data-testid=cwm-status]");
  await page.waitForSelector(".cwm-status-popup");
};
const contextMenu = (theme) => async (page) => {
  await page.waitForSelector("[data-testid=cwm-status]");
  await page.dblclick('.tree-row[data-path="app"]');
  if (theme === "light") await light(page);
  await page.click('.tree-row[data-path="app/Models"]', { button: "right" });
  await page.waitForSelector(".context-menu");
};

/** The host asked everyone to follow: the guest shows the host's file with the banner. */
const followHost = (theme) => async (page) => {
  await page.waitForSelector("[data-testid=cwm-status]");
  if (theme === "light") await light(page);
  await page.waitForSelector(".editor-slot:not(.hidden) .cwm-follow-banner:not([hidden])");
  await page.waitForFunction(() => document.querySelector(".editor-slot:not(.hidden) .cm-content")?.textContent.includes("OrderController"));
  await page.waitForTimeout(400);
};
const following = (theme) => {
  const fn = followHost(theme);
  fn.query = "?project=cwm-guest&permission=readOnly&follow=all";
  fn.waitFor = "[data-testid=cwm-status]";
  return fn;
};

/** The host shares its terminal; a Read-only guest watches it (S5). */
const sharedTerminal = (theme) => {
  const fn = async (page) => {
    await page.waitForSelector("[data-testid=cwm-status]");
    if (theme === "light") await light(page);
    await page.keyboard.press("Alt+F12");
    await page.waitForSelector('.term-tab[data-terminal-tab="Local"]');
    await page.waitForSelector(".term-host:not(.hidden) .xterm-screen");
    await page.waitForTimeout(800);
  };
  fn.query = "?project=cwm-guest&permission=readOnly&share";
  fn.waitFor = "[data-testid=cwm-status]";
  return fn;
};

/** The host's run of UserTest in a Full access guest's Run tool window, with the Run widget. */
const sharedRun = (theme) => {
  const fn = async (page) => {
    await page.waitForSelector("[data-testid=cwm-status]");
    if (theme === "light") await light(page);
    await page.click('[data-tool-window="run"]');
    await page.waitForSelector('.run-tab:has-text("UserTest")');
    await page.waitForSelector(".test-tree");
    await page.waitForTimeout(3500);
  };
  fn.query = "?project=cwm-guest&permission=full&share";
  fn.waitFor = "[data-testid=cwm-status]";
  return fn;
};

/** A file over the co-editing limit opens read-only for an Edit files guest, with a bar saying why. */
const viewOnly = (theme) => {
  const fn = async (page) => {
    await page.waitForSelector("[data-testid=cwm-status]");
    if (theme === "light") await light(page);
    await page.waitForSelector(".editor-slot:not(.hidden) .cwm-view-only-banner");
    await page.waitForTimeout(400);
  };
  // The guest follows the host into its log.
  fn.query = "?project=cwm-guest&permission=edit&follow=all&log";
  fn.waitFor = "[data-testid=cwm-status]";
  return fn;
};

/** A Full access guest's Code With Me tool window: the path to the host and its round-trip time. */
const toolWindow = (theme) => {
  const fn = async (page) => {
    await page.waitForSelector("[data-testid=cwm-status]");
    if (theme === "light") await light(page);
    await page.evaluate(() => window.__forgeRunCommand("CodeWithMe.ShowToolWindow"));
    await page.waitForSelector('.cwm-tool-window .cwm-participant[data-participant="0"] .cwm-path[data-path="direct"]');
    await page.waitForTimeout(300);
  };
  fn.query = "?project=cwm-guest&permission=full";
  fn.waitFor = "[data-testid=cwm-status]";
  return fn;
};

const withQuery = (fn, permission) => {
  fn.query = `?project=cwm-guest&permission=${permission}`;
  return fn;
};

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "cwm-guest-terminal": sharedTerminal("dark"),
  "cwm-guest-terminal-light": sharedTerminal("light"),
  "cwm-guest-run": sharedRun("dark"),
  "cwm-guest-run-light": sharedRun("light"),
  "cwm-guest-follow": following("dark"),
  "cwm-guest-follow-light": following("light"),
  "cwm-guest-project": withQuery(project("dark"), "edit"),
  "cwm-guest-project-light": withQuery(project("light"), "edit"),
  "cwm-guest-readonly-menu": withQuery(contextMenu("dark"), "readOnly"),
  "cwm-guest-readonly-menu-light": withQuery(contextMenu("light"), "readOnly"),
  "cwm-guest-search": withQuery(search("dark"), "readOnly"),
  "cwm-guest-search-light": withQuery(search("light"), "readOnly"),
  "cwm-guest-hover": withQuery(hover("dark"), "readOnly"),
  "cwm-guest-hover-light": withQuery(hover("light"), "readOnly"),
  "cwm-guest-status": withQuery(status("dark"), "readOnly"),
  "cwm-guest-status-light": withQuery(status("light"), "readOnly"),
  "cwm-guest-view-only": viewOnly("dark"),
  "cwm-guest-view-only-light": viewOnly("light"),
  "cwm-guest-tool-window": toolWindow("dark"),
  "cwm-guest-tool-window-light": toolWindow("light"),
};
