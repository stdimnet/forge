/** Settings | Tools | AI Providers. */
async function openAiSettings(page) {
  await page.evaluate(() => window.__forgeRunCommand("ai.settings"));
  await page.waitForSelector(".ai-row[data-provider]");
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "ai-providers-settings": async (page) => {
    await openAiSettings(page);
    // Fetch the selected provider's models so the default-model dropdown is populated.
    await page.click("text=Fetch Models");
    await page.waitForSelector("text=models available");
    await page.click(".ai-detail-pane .ai-combo-btn");
    await page.hover(".ai-combo-item >> nth=0");
  },
  "ai-providers-add-preset": async (page) => {
    await openAiSettings(page);
    await page.click("[data-testid=ai-add-provider]");
    await page.waitForSelector("[data-testid=ai-preset-picker]");
    await page.hover("[data-preset=lmstudio]");
  },
  "ai-providers-test-connection": async (page) => {
    await openAiSettings(page);
    await page.click(".ai-row[data-provider=ollama]");
    await page.click("[data-testid=ai-test]");
    await page.waitForSelector(".ai-chip.ok");
  },
  "ai-providers-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openAiSettings(page);
    await page.click("[data-testid=ai-test]");
    await page.waitForSelector(".ai-chip.ok");
  },
};
