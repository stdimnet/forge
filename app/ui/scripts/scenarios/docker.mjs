/** Services tool window: Docker containers with logs, compose services, Kubernetes pods, settings. */

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + 200, y);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, 900 - 26 - height, { steps: 4 });
  await page.mouse.up();
}

/** The ".env" data source suggestion would cover the details pane. */
async function closeBalloons(page) {
  await page.waitForSelector(".balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const b of await page.locator(".balloon-close").all()) await b.click().catch(() => undefined);
}

/** @param {import("playwright-core").Page} page */
async function openServices(page) {
  await closeBalloons(page);
  await page.keyboard.press("Alt+8");
  await page.waitForSelector('.svc-row[data-node-id="docker/containers"]');
  await bottomHeight(page, 470);
  await page.waitForTimeout(150);
}

const row = (id) => `.svc-row[data-node-id="${id}"]`;

/** @param {import("playwright-core").Page} page */
async function selectByLabel(page, label) {
  await page.locator(".svc-row", { has: page.locator(`.tree-label:text-is("${label}")`) }).first().click();
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "services-docker-logs": async (page) => {
    await openServices(page);
    await selectByLabel(page, "acme-shop-app-1");
    await page.click('.svc-tab[data-tab="log"]');
    await page.waitForSelector(".svc-log .run-line");
    await page.waitForTimeout(2300);
    await page.focus(".svc-tree");
  },
  "services-docker-dashboard": async (page) => {
    await openServices(page);
    await selectByLabel(page, "acme-shop-app-1");
    await page.waitForSelector('.svc-section[data-section="Port Bindings"]');
    await page.focus(".svc-tree");
  },
  "services-exec-terminal": async (page) => {
    await openServices(page);
    await selectByLabel(page, "acme-shop-app-1");
    await page.click('.svc-tool[data-action="exec"]');
    await page.waitForSelector('.term-tab[data-terminal-tab="acme-shop-app-1"]');
    await page.waitForTimeout(300);
    for (const cmd of ["php -v", "env"]) {
      await page.keyboard.type(cmd, { delay: 8 });
      await page.keyboard.press("Enter");
      await page.waitForTimeout(150);
    }
  },
  "services-attached-console": async (page) => {
    await openServices(page);
    await selectByLabel(page, "acme-shop-redis-1");
    await page.click('.svc-tab[data-tab="console"]');
    await page.waitForSelector(".svc-attach .xterm-screen");
    await page.waitForTimeout(400);
  },
  "services-pull-image": async (page) => {
    await openServices(page);
    await page.click(`${row("docker/containers")} .tree-chevron`);
    await page.click(`${row("docker/images")} .tree-chevron`);
    await page.click(row("docker/images"));
    await page.click('.svc-tool[data-action="pull"]');
    await page.waitForSelector(".git-dialog input");
    await page.keyboard.type("postgres:17-alpine");
    await page.keyboard.press("Enter");
    await page.waitForSelector('.svc-tab.active[data-tab="log"]');
    await page.waitForTimeout(1800);
    await page.focus(".svc-tree");
  },
  "services-light-theme": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openServices(page);
    await selectByLabel(page, "acme-shop-app-1");
    await page.click('.svc-tab[data-tab="log"]');
    await page.waitForSelector(".svc-log .run-line");
    await page.focus(".svc-tree");
  },
  "services-compose": async (page) => {
    await openServices(page);
    await page.click(`${row("docker/containers")} .tree-chevron`);
    await page.click(row("compose/acme-shop"));
    await page.waitForSelector('.svc-section[data-section="Services"]');
    await page.click('.svc-tool[data-action="restart"]');
    await page.waitForSelector('.svc-tab.active[data-tab="log"]');
    await page.waitForTimeout(1400);
    await page.click(row("compose/acme-shop/redis"));
    await page.click(row("compose/acme-shop"));
    await page.focus(".svc-tree");
  },
  "services-compose-dashboard": async (page) => {
    await openServices(page);
    await page.click(`${row("docker/containers")} .tree-chevron`);
    await page.click(row("compose/acme-shop"));
    await page.waitForSelector('.svc-section[data-section="Services"]');
    await page.click(row("compose/acme-shop/mysql"));
    await page.click('.svc-tool[data-action="stop"]');
    await page.waitForTimeout(900);
    await page.click(row("compose/acme-shop"));
    await page.click('.svc-tab[data-tab="dashboard"]');
    await page.waitForSelector('.svc-section[data-section="Services"]');
    await page.focus(".svc-tree");
  },
  "services-kubernetes": async (page) => {
    await openServices(page);
    await page.click(`${row("docker/containers")} .tree-chevron`);
    await page.click(`${row("compose/acme-shop")} .tree-chevron`);
    await page.click(`${row("kube/kind-acme")} .tree-chevron`);
    await page.waitForSelector(row("kube/kind-acme/shop/pods/web-7d4b9c6f8-x2kqf"));
    await page.click(`${row("kube/kind-acme/shop")}`);
    await page.waitForSelector('.svc-section[data-section="Pods"]');
    await page.focus(".svc-tree");
  },
  "services-kubernetes-pod-log": async (page) => {
    await openServices(page);
    await page.click(`${row("docker/containers")} .tree-chevron`);
    await page.click(`${row("compose/acme-shop")} .tree-chevron`);
    await page.click(`${row("kube/kind-acme")} .tree-chevron`);
    await page.click(row("kube/kind-acme/shop/pods/scheduler-6f8d9c7b5-tq9vd"));
    await page.click('.svc-tab[data-tab="log"]');
    await page.waitForSelector(".svc-log .run-line");
    await page.focus(".svc-tree");
  },
  "services-compose-gutter": async (page) => {
    await closeBalloons(page);
    await page.locator('.tree-row[data-path="docker-compose.yml"]').dblclick();
    await page.waitForSelector('.editor-tab.active[title="docker-compose.yml"]');
    await page.waitForSelector(".cm-line-marker.marker-run");
    await page.locator(".cm-line-marker.marker-run").nth(2).click();
    await page.waitForSelector(".run-popup-menu");
  },
  "settings-docker": async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.locator(".set-tree-row", { hasText: "Docker" }).first().click();
    await page.click('[data-testid="docker-test-connection"]');
    await page.waitForSelector(".svc-test-result.ok");
  },
  "settings-kubernetes": async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.locator(".set-tree-row", { hasText: "Kubernetes" }).first().click();
    await page.waitForTimeout(200);
  },
};
