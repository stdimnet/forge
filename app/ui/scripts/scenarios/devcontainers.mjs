/**
 * Dev Containers and WSL: Welcome screen > Remote Development > Dev Containers with the
 * containers Docker knows, the build log of "Create Dev Container and Mount Sources", the
 * devcontainer.json gutter icon, and the WSL distributions with the folder chooser.
 */

const welcome = (page, fn, waitFor) => Object.assign(fn, { query: `?welcome=${page}`, waitFor });

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "devcontainers-welcome": welcome(
    "devcontainers",
    async (page) => {
      await page.waitForSelector('[data-container="a81e44c0f6d2"]');
      await page.hover('[data-container="3f9c2a1b7d4e"]');
    },
    "[data-testid=welcome-devcontainers]",
  ),

  "devcontainers-config": welcome(
    "devcontainers",
    async (page) => {
      await page.click("[data-testid=devcontainer-new]");
      await page.waitForSelector("[data-testid=devcontainer-summary]");
    },
    "[data-testid=welcome-devcontainers]",
  ),

  "devcontainers-build-log": welcome(
    "devcontainers",
    async (page) => {
      await page.click("[data-testid=devcontainer-new]");
      await page.waitForSelector("[data-testid=devcontainer-summary]");
      await page.click("[data-testid=devcontainer-build]");
      await page.waitForFunction(
        () => [...document.querySelectorAll(".devc-log-line")].some((l) => l.textContent?.includes("Package operations")),
        undefined,
        { timeout: 15000 },
      );
    },
    "[data-testid=welcome-devcontainers]",
  ),

  "devcontainers-gutter": async (page) => {
    await page.waitForSelector(".balloon-close", { timeout: 3000 }).catch(() => undefined);
    for (const b of await page.locator(".balloon-close").all()) await b.click().catch(() => undefined);
    await page.click('.tree-row[data-path=".devcontainer"]');
    await page.click('.tree-row[data-path=".devcontainer/devcontainer.json"]');
    await page.waitForSelector('.editor-tab.active[title=".devcontainer/devcontainer.json"]');
    await page.waitForSelector(".cm-line-markers .marker-run");
    await page.click(".cm-line-markers .marker-run");
    await page.waitForSelector(".popup-menu, .run-popup-menu, [role=menu]");
  },

  "devcontainers-project": welcome(
    "devcontainers",
    async (page) => {
      await page.click('[data-container="3f9c2a1b7d4e"]');
      await page.waitForSelector("[data-testid=remote-status]");
      await page.click("[data-testid=remote-status]");
      await page.waitForSelector(".remote-popup");
    },
    "[data-testid=welcome-devcontainers]",
  ),

  "wsl-distros": welcome("wsl", async (page) => page.waitForSelector('[data-distro="Debian"]'), "[data-testid=welcome-wsl]"),

  "wsl-folder-chooser": welcome(
    "wsl",
    async (page) => {
      await page.click('[data-distro="Ubuntu-24.04"]');
      await page.waitForSelector('[data-testid=remote-chooser] [data-path="/home/demo/projects"]');
      await page.dblclick('[data-path="/home/demo/projects"]');
      await page.waitForSelector('[data-path="/home/demo/projects/acme-shop"]');
      await page.click('[data-path="/home/demo/projects/acme-shop"]');
    },
    "[data-testid=welcome-wsl]",
  ),
};
