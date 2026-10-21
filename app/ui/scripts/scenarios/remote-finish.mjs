/**
 * Every feature in a remote project (`?project=remote`): the npm tool window and compose run
 * on the host, an HTTP request sent from the host through an SSH tunnel, and a language
 * server installed on the host from the Language Servers settings page.
 */

/** Close the balloons (data source suggestion, …) so they do not cover the panels. */
async function closeBalloons(page) {
  await page.waitForSelector(".balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const b of await page.locator(".balloon-close").all()) await b.click().catch(() => undefined);
}

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  await page.mouse.move(box.x + 200, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, 900 - 26 - height, { steps: 4 });
  await page.mouse.up();
}

/** Open a project file, expanding only the folders that are still collapsed. */
async function openFile(page, path) {
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    const child = page.locator(`.tree-row[data-path^="${dir}/"]`).first();
    if (!(await child.isVisible().catch(() => false))) await page.click(`.tree-row[data-path="${dir}"]`);
  }
  await page.click(`.tree-row[data-path="${path}"]`);
  await page.waitForSelector(`.editor-tab.active[title="${path}"]`);
}

/** Run output contains `text`. */
async function waitForOutput(page, text) {
  await page.waitForFunction((t) => [...document.querySelectorAll(".run-line")].some((l) => l.textContent?.includes(t)), text, {
    timeout: 10000,
  });
}

const remote = (fn, extra = "") => Object.assign(fn, { query: `?project=remote${extra}`, waitFor: "[data-testid=remote-status]" });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "remote-finish-npm": remote(async (page) => {
    await closeBalloons(page);
    if (!(await page.locator('[data-tool-window-panel="npm"]').isVisible().catch(() => false))) {
      await page.click('[data-tool-window="npm"]');
    }
    await page.waitForSelector(".npm-script");
    await page.locator('.npm-script[data-script="dev"]').first().dblclick();
    await page.waitForSelector(".run-line");
    await page.waitForTimeout(800);
  }),

  "remote-finish-compose": remote(async (page) => {
    await closeBalloons(page);
    await page.keyboard.press("Alt+8");
    await page.waitForSelector('.svc-row[data-node-id="docker/containers"]');
    await bottomHeight(page, 470);
    const project = page.locator('.svc-row[data-node-id^="compose/"]').first();
    await project.click();
    await project.click({ button: "right" });
    await page.locator('.run-menu-item:has(.run-menu-label:text-is("Up"))').first().click();
    await page.waitForFunction(() => document.body.textContent?.includes("Running on deploy@shop.example.com"), null, { timeout: 10000 });
    await page.waitForTimeout(1500);
  }),

  "remote-finish-http": remote(async (page) => {
    await closeBalloons(page);
    await openFile(page, "http/orders.http");
    await page.waitForSelector(".http-toolbar");
    await page.waitForSelector(".cm-line-marker.marker-run");
    await page.evaluate(() => {
      const view = document.querySelector(".editor-slot:not(.hidden) .cm-content")?.cmTile?.view;
      view.dispatch({ selection: { anchor: view.state.doc.line(2).from } });
    });
    await page.evaluate(() => window.__forgeRunCommand("RunClass"));
    await waitForOutput(page, "Tests:");
    await bottomHeight(page, 500);
    // Show the request and the tunnel line instead of the end of the response.
    await page.evaluate(() => {
      let el = document.querySelector(".run-line")?.parentElement;
      while (el && el.scrollHeight <= el.clientHeight) el = el.parentElement;
      if (el) el.scrollTop = 0;
    });
  }),

  "remote-finish-langserver": remote(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("langservers.settings"));
    await page.waitForSelector('[data-testid="ls-remote-note"]');
    await page.evaluate(() => {
      window.__forgeMockInstallHold = 0.55;
    });
    await page.click('[data-testid="install-docker-compose-langserver"]');
    await page.waitForSelector('.ls-tr[data-server="docker-compose-langserver"] .ls-installing');
    await page.waitForTimeout(1500);
    // The note naming the host, with the download in the status bar.
    await page.locator(".set-content").evaluate((el) => (el.scrollTop = 0));
  }),

  "remote-finish-langserver-dialog": remote(async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("langservers.settings"));
    await page.waitForSelector('[data-testid="ls-remote-note"]');
    await page.click('.ls-tr[data-server="pyright"]');
    await page.locator('.ls-link:has-text("Other ways")').first().click();
    await page.waitForSelector(".ls-install-dialog");
    await page.click('.ls-install-dialog .btn-primary:has-text("Install")');
    await page.waitForFunction(() => document.querySelector('[data-testid="install-log"]')?.textContent?.includes("Installing on"));
    await page.waitForTimeout(1200);
  }),
};
