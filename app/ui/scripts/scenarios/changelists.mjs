/**
 * Changelists, partial commits, commit checks and patches: the changelists in the Commit tool
 * window with their context menu, Move to Another Changelist (F6), the hunk checkboxes in the
 * editor gutter and in the commit diff, the Commit Options (gear) and Create / Apply Patch.
 */
const run = (page, id) => page.evaluate((cmd) => window.__forgeRunCommand(cmd), id);
const USER = "app/Models/User.php";

async function openCommit(page) {
  await page.keyboard.press("Control+k");
  await page.waitForSelector('.commit-group[data-changelist="cl1"]');
  await page.waitForSelector(`.commit-row[data-path="${USER}"]`);
}

/** Move the first change of User.php to the "Checkout refactoring" changelist. */
async function splitUserFile(page) {
  await page.evaluate(async (path) => {
    const { changesApi } = await import("/src/features/git/changesApi.ts");
    const { refreshChangelists } = await import("/src/features/git/changelists.ts");
    const hunks = await changesApi.fileHunks(path);
    await changesApi.move([path], "cl1", [hunks[0].id]);
    await refreshChangelists();
  }, USER);
  await page.waitForSelector(`.commit-row[data-changelist="cl1"][data-path="${USER}"]`);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "git-changelists": async (page) => {
    await openCommit(page);
    await splitUserFile(page);
    await page.click(`.commit-row[data-changelist="default"][data-path="${USER}"]`, { button: "right" });
    await page.waitForSelector(".context-menu, [role=menu]");
    await page.waitForTimeout(250);
  },
  "git-move-to-changelist": async (page) => {
    await openCommit(page);
    await page.click(`.commit-row[data-path="routes/web.php"]`);
    await page.keyboard.press("F6");
    await page.waitForSelector(".gd-move-changelist");
    await page.click(".gd-move-changelist input[type=radio] >> nth=1");
    await page.fill(".gd-move-changelist .gd-input", "Routing");
    await page.click(".gd-move-changelist .git-dialog-check input");
    await page.waitForTimeout(200);
  },
  "git-partial-commit-gutter": async (page) => {
    await openCommit(page);
    await page.waitForSelector(".cm-git-hunk-check");
    // Exclude the second change of the file from the commit.
    await page.click(".cm-git-hunk-check >> nth=1");
    await page.waitForFunction(() => document.querySelectorAll(".cm-git-hunk-check:not(:checked)").length === 1);
    await page.waitForTimeout(250);
  },
  "git-partial-commit-diff": async (page) => {
    await openCommit(page);
    await page.dblclick(`.commit-row[data-path="${USER}"]`);
    await page.waitForSelector(".diff-viewer .cm-git-hunk-check");
    await page.click(".diff-viewer .cm-git-hunk-check >> nth=0");
    await page.waitForTimeout(300);
  },
  "git-commit-options": async (page) => {
    await openCommit(page);
    await page.fill(".commit-message textarea", "Add totalSpent helper to User");
    await page.click(".commit-options-btn");
    await page.waitForSelector(".commit-options-popup");
    await page.click(".commit-options-popup label:has-text('Reformat code') input");
    await page.click(".commit-options-popup label:has-text('Analyze code') input");
    await page.click(".commit-options-popup label:has-text('Check TODO') input");
    await page.waitForTimeout(250);
  },
  "git-create-patch": async (page) => {
    await openCommit(page);
    await page.click('.commit-group[data-changelist="default"]', { button: "right" });
    await page.click(".context-menu >> text=Create Patch");
    await page.waitForSelector(".gd[role=dialog]");
    await page.waitForTimeout(200);
  },
  "git-apply-patch": async (page) => {
    await openCommit(page);
    await page.evaluate(async () => {
      const { changesApi } = await import("/src/features/git/changesApi.ts");
      const { previewPatch } = await import("/src/features/git/changesDialogs.ts");
      const patch =
        (await changesApi.createPatch(["routes/web.php", "app/Http/Controllers/OrderController.php"])) +
        "diff --git a/app/Support/Money.php b/app/Support/Money.php\nnew file mode 100644\n--- /dev/null\n+++ b/app/Support/Money.php\n@@ -0,0 +1,3 @@\n+<?php\n+\n+namespace App\\Support;\n" +
        "--- a/README.md\n+++ b/README.md\n@@ -1,1 +1,1 @@\n-# Old title that is not there\n+# Acme Shop\n";
      await previewPatch(patch, "orders-cleanup");
    });
    await page.waitForSelector(".gd-apply-patch .gd-file-indent");
    await page.waitForTimeout(250);
  },
};

scenarios["git-partial-commit-done"] = async (page) => {
  await openCommit(page);
  await page.waitForSelector(".cm-git-hunk-check");
  // Only User.php, without its second change.
  await page.click('.commit-group[data-changelist="default"] .commit-check');
  await page.click(`.commit-row[data-path="${USER}"] .commit-check`);
  await page.click(".cm-git-hunk-check >> nth=1");
  await page.fill(".commit-message textarea", "Allow mass-assigning the password");
  await page.click(".commit-actions .btn-primary");
  await page.waitForSelector("text=committed (1 partly)");
  await page.waitForTimeout(400);
};

scenarios["git-changelists-light"] = async (page) => {
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await scenarios["git-commit-options"](page);
};

export default scenarios;
