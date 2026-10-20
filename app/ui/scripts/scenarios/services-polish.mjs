/** Services polish: Kubernetes Forward Ports, the pod container picker, deployment logs and Docker over SSH / Docker contexts. */

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

async function closeBalloons(page) {
  await page.waitForSelector(".balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const b of await page.locator(".balloon-close").all()) await b.click().catch(() => undefined);
}

const row = (id) => `.svc-row[data-node-id="${id}"]`;
const WEB_POD = "kube/kind-acme/shop/pods/web-7d4b9c6f8-x2kqf";

/** Services with the kind cluster's shop namespace expanded (Docker and Compose collapsed). */
async function openKube(page) {
  await closeBalloons(page);
  await page.keyboard.press("Alt+8");
  await page.waitForSelector(row("docker/containers"));
  await bottomHeight(page, 500);
  await page.click(`${row("docker/containers")} .tree-chevron`);
  await page.click(`${row("compose/acme-shop")} .tree-chevron`);
  await page.click(`${row("kube/kind-acme")} .tree-chevron`);
  await page.waitForSelector(row(WEB_POD));
  await page.click(`${row("kube/kind-acme/shop/services")} .tree-chevron`).catch(() => undefined);
}

/** @param {import("playwright-core").Page} page */
async function forwardPorts(page, spec, pod = WEB_POD) {
  await page.click(row(pod));
  await page.waitForSelector('.svc-tool[data-action="forward"]:not([disabled])');
  await page.click('.svc-tool[data-action="forward"]');
  await page.waitForSelector('[data-testid="forward-ports-dialog"]');
  if (spec !== undefined) {
    await page.fill('[data-testid="forward-spec"]', spec);
    await page.keyboard.press("Enter");
    await page.waitForSelector('[data-testid="forward-ports-dialog"]', { state: "detached" });
  }
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "services-port-forward-dialog": async (page) => {
    await openKube(page);
    await forwardPorts(page);
    await page.locator(".svc-chip", { hasText: "http" }).click();
    await page.waitForTimeout(150);
  },
  "services-port-forwards": async (page) => {
    await openKube(page);
    await forwardPorts(page, "8080:80, 9000");
    await closeBalloons(page);
    await page.waitForSelector('.svc-forwards tr[data-forward]');
    await page.locator(".svc-forwards").scrollIntoViewIfNeeded();
    await page.focus(".svc-tree");
    await page.waitForTimeout(200);
  },
  "services-port-forwards-all": async (page) => {
    await openKube(page);
    await forwardPorts(page, "8080:80");
    await forwardPorts(page, "13306:3306", "kube/kind-acme/shop/pods/mysql-0");
    await closeBalloons(page);
    await page.click(row("kube"));
    await page.waitForSelector('.svc-forwards tr[data-forward]');
    await page.focus(".svc-tree");
  },
  "services-container-picker": async (page) => {
    await openKube(page);
    await page.click(row(WEB_POD));
    await page.click('.svc-tab[data-tab="log"]');
    await page.waitForSelector('[data-testid="container-picker"]');
    await page.selectOption('[data-testid="container-picker"]', "nginx");
    await page.waitForSelector(".svc-log .run-line");
    await page.waitForTimeout(300);
    await page.focus(".svc-tree");
  },
  "services-deployment-logs": async (page) => {
    await openKube(page);
    await page.click(`${row("kube/kind-acme/shop/pods")} .tree-chevron`);
    await page.click(`${row("kube/kind-acme/shop/deployments")} .tree-chevron`);
    await page.click(row("kube/kind-acme/shop/deployments/web"));
    await page.click('.svc-tab[data-tab="log"]');
    await page.waitForSelector(".svc-log .run-line");
    await page.waitForTimeout(300);
    await page.focus(".svc-tree");
  },
  "services-docker-contexts": async (page) => {
    await closeBalloons(page);
    await page.keyboard.press("Alt+8");
    await page.waitForSelector(row("docker/containers"));
    await bottomHeight(page, 500);
    await page.click(row("docker"));
    await page.waitForSelector('.svc-section[data-section="Docker Contexts"]');
    await page.focus(".svc-tree");
  },
  "settings-docker-ssh": async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.locator(".set-tree-row", { hasText: "Docker" }).first().click();
    await page.selectOption('[data-setting="docker.connection"] select', "ssh");
    await page.fill('[data-setting="docker.ssh.url"] input', "ssh://deploy@build.acme.dev");
    await page.click('[data-testid="docker-test-connection"]');
    await page.waitForSelector(".svc-test-result.ok");
  },
  "settings-docker-contexts": async (page) => {
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.locator(".set-tree-row", { hasText: "Docker" }).first().click();
    await page.waitForSelector('[data-testid="docker-contexts"]');
    await page.locator('[data-testid="docker-contexts"] button', { hasText: "Use" }).first().click();
    await page.click('[data-testid="docker-test-connection"]');
    await page.waitForSelector(".svc-test-result.ok");
    await page.locator('[data-testid="docker-contexts"]').scrollIntoViewIfNeeded();
  },
  "services-port-forwards-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openKube(page);
    await forwardPorts(page, "8080:80");
    await closeBalloons(page);
    await page.click('.svc-tab[data-tab="log"]');
    await page.waitForSelector(".svc-log .run-line");
    await page.click('.svc-tab[data-tab="dashboard"]');
    await page.waitForSelector('.svc-forwards tr[data-forward]');
    await page.focus(".svc-tree");
  },
};
