/**
 * Kubernetes contexts from the project's host: the remote demo project (`?project=remote`)
 * shows the contexts of its host's kubeconfig with the context / namespace selector, applies
 * a manifest with the host's kubectl and sets the kubectl path per host.
 */

/** Drag the bottom splitter so the bottom tool window gets `height` pixels. */
async function bottomHeight(page, height) {
  const box = await page.locator(".splitter-h").boundingBox();
  if (!box) return;
  await page.mouse.move(box.x + 200, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + 200, 900 - 26 - height, { steps: 4 });
  await page.mouse.up();
}

async function closeBalloons(page) {
  await page.waitForSelector(".balloon-close", { timeout: 3000 }).catch(() => undefined);
  for (const b of await page.locator(".balloon-close").all()) await b.click().catch(() => undefined);
}

/** Trust the remote project when it opened in Safe Mode. */
async function trust(page) {
  const dialog = page.locator('[data-testid="trust-dialog"]');
  if (await dialog.isVisible({ timeout: 1500 }).catch(() => false)) {
    await dialog.locator("button", { hasText: /^Trust/ }).first().click();
  }
}

const row = (id) => `.svc-row[data-node-id="${id}"]`;
const remote = (fn) => Object.assign(fn, { query: "?project=remote", waitFor: "[data-testid=remote-status]" });

/** Services with Docker and Compose collapsed and the host's k3s context expanded. */
async function openKube(page) {
  await trust(page);
  await closeBalloons(page);
  await page.keyboard.press("Alt+8");
  await page.waitForSelector(row("docker/containers"));
  await bottomHeight(page, 500);
  await page.click(`${row("docker")} .tree-chevron`);
  await page.click(`${row("compose")} .tree-chevron`);
  await page.click(`${row("kube/k3s-shop")} .tree-chevron`);
  await page.waitForSelector('.svc-row[data-node-id^="kube/k3s-shop/shop/pods/"]');
  await page.waitForSelector('[data-testid="kube-selector"]');
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "services-kube-host-contexts": remote(async (page) => {
    await openKube(page);
    await page.click(row("kube"));
    await page.waitForSelector('.svc-section[data-section="Kubernetes"]').catch(() => undefined);
    await page.focus(".svc-tree");
    await page.waitForTimeout(300);
  }),

  "services-kube-host-contexts-light": remote(async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openKube(page);
    await page.click('.svc-row[data-node-id^="kube/k3s-shop/shop/pods/web-"]');
    await page.focus(".svc-tree");
    await page.waitForTimeout(300);
  }),

  "services-kube-apply": remote(async (page) => {
    await openKube(page);
    await page.click('.tree-row[data-path="k8s"]');
    await page.click('.tree-row[data-path="k8s/deployment.yaml"]');
    await page.waitForSelector('.editor-tab.active[title="k8s/deployment.yaml"]');
    await page.waitForSelector(".cm-line-marker.marker-run");
    await page.locator(".cm-line-marker.marker-run").first().click();
    await page.locator('.run-menu-item:has(.run-menu-label:text-matches("^Apply"))').first().click();
    await page.waitForFunction(() => document.body.textContent?.includes("deployment.apps/web configured"), null, { timeout: 10000 });
    await page.waitForTimeout(800);
  }),

  "settings-kubernetes-hosts": remote(async (page) => {
    await trust(page);
    await closeBalloons(page);
    await page.evaluate(() => window.__forgeRunCommand("settings.open"));
    await page.waitForSelector(".set-dialog");
    await page.locator(".set-tree-row", { hasText: "Kubernetes" }).first().click();
    await page.waitForSelector('[data-testid="kubectl-hosts"]');
    await page.fill('[data-testid="kubectl-hosts"] input[placeholder="/usr/local/bin/kubectl"]', "/usr/local/bin/k3s kubectl");
    await page.locator('[data-testid="kubectl-hosts"] button', { hasText: "Add" }).click();
    await page.locator('[data-testid="kubectl-hosts"]').scrollIntoViewIfNeeded();
    await page.waitForTimeout(200);
  }),
};
