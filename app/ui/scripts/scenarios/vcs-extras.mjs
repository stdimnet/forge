/**
 * Get from Version Control (Repository URL with Test, an account's repositories, the clone
 * progress), the Worktrees tab and New Worktree…, the worktree marker in the branches popup,
 * Commit Message History (Ctrl+M), signature icons in the log and Configure GPG Key….
 */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "git-clone-url": async (page) => {
    await run(page, "Git.Clone");
    await page.waitForSelector(".clone-dialog [data-testid=clone-url]");
    await page.fill("[data-testid=clone-url]", "git@github.com:acme/shop-admin.git");
    await page.click(".clone-dialog button:has-text('Test')");
    await page.waitForSelector(".clone-test-ok");
    await page.waitForTimeout(300);
  },
  "git-clone-account": async (page) => {
    await run(page, "Git.Clone");
    await page.waitForSelector(".clone-dialog");
    await page.click("[data-clone-tab=forgejo]");
    await page.waitForSelector(".clone-repo");
    await page.fill("[data-testid=clone-search]", "shop");
    await page.click(".clone-repo >> nth=0");
    await page.waitForTimeout(300);
  },
  "git-clone-progress": async (page) => {
    await run(page, "Git.Clone");
    await page.waitForSelector("[data-testid=clone-url]");
    await page.fill("[data-testid=clone-url]", "https://git.acme.dev/acme/design-system.git");
    await page.waitForTimeout(200);
    await page.click("[data-testid=clone-ok]");
    await page.waitForFunction(() => document.querySelector(".clone-progress-text")?.textContent?.includes("Receiving"));
    await page.waitForTimeout(150);
  },
  "git-worktrees": async (page) => {
    await run(page, "Git.Show.Worktrees");
    await page.waitForSelector(".worktrees-row[data-path]");
    await page.click(".worktrees-row[data-path='/home/marlon/src/acme-shop-checkout-flow']");
    await page.waitForTimeout(300);
  },
  "git-new-worktree": async (page) => {
    await run(page, "Git.Show.Worktrees");
    await page.waitForSelector(".worktrees-row[data-path]");
    await run(page, "Git.Worktree.Create");
    await page.waitForSelector("[data-testid=worktree-branch]");
    await page.fill("[data-testid=worktree-branch]", "feature/invoice-pdf");
    await page.waitForTimeout(300);
  },
  "git-branches-worktree": async (page) => {
    await run(page, "git.branches");
    await page.waitForSelector("[data-testid=branch-worktree]");
    await page.hover(".branch-row[data-branch='feature/checkout-flow']");
    await page.waitForSelector(".branch-submenu");
    await page.waitForTimeout(300);
  },
  "git-message-history": async (page) => {
    await page.keyboard.press("Control+k");
    await page.waitForSelector(".commit-message textarea");
    await page.focus(".commit-message textarea");
    await page.keyboard.press("Control+m");
    await page.waitForSelector("[data-testid=message-history] .message-history-item");
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(300);
  },
  "git-log-signatures": async (page) => {
    await run(page, "git.log");
    await page.waitForSelector(".log-row .log-signature");
    await page.click(".log-row >> nth=1");
    await page.waitForSelector("[data-testid=log-signer]");
    await page.waitForTimeout(300);
  },
  "git-signing-settings": async (page) => {
    await page.evaluate(async () => {
      const { openSettings } = await import("/src/features/settings/dialogState.ts");
      openSettings("vcs.git");
    });
    await page.waitForSelector("[data-testid=signing-configure]");
    await page.click("[data-testid=signing-configure]");
    await page.waitForSelector("[data-signing-key^='openpgp:']");
    await page.click("[data-signing-key='ssh:/home/marlon/.ssh/id_ed25519.pub'] input");
    await page.waitForTimeout(300);
  },
};

const welcome = async (page) => {
  await page.click("[data-testid=welcome-vcs]");
  await page.waitForSelector(".clone-dialog [data-testid=clone-url]");
  await page.fill("[data-testid=clone-url]", "https://github.com/acme/shop.git");
  await page.waitForTimeout(300);
};
scenarios["welcome-get-from-vcs"] = Object.assign(welcome, { query: "?welcome", waitFor: ".welcome" });

const light = async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await scenarios["git-clone-account"](page);
};
scenarios["git-clone-light"] = light;

export default scenarios;
