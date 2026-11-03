/** MCP Server: settings page with client configurations, the status bar popup with the call log, an approval card. */

/** @param {import("playwright-core").Page} page */
async function openMcpSettings(page) {
  await page.evaluate(() => window.__forgeRunCommand("Mcp.Settings"));
  await page.waitForSelector('[data-testid="mcp-page"]');
  await page.waitForSelector('[data-testid="mcp-client-configs"] pre');
}

/** @type {Record<string, ((page: import("playwright-core").Page) => Promise<void>) & { query?: string }>} */
const scenarios = {
  "mcp-server-settings": async (page) => {
    await openMcpSettings(page);
  },
  "mcp-server-popup": async (page) => {
    await page.click('[data-testid="mcp-status"]');
    await page.waitForSelector('[data-testid="mcp-popup"] .mcp-log-row');
  },
  "mcp-server-approval": async (page) => {
    await page.waitForSelector('[data-testid="mcp-approvals"] .mcp-approval', { timeout: 10000 });
  },
  "mcp-server-settings-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openMcpSettings(page);
    await page.click('.mcp-client-tab:has-text("Claude Desktop")');
  },
};
scenarios["mcp-server-settings"].query = "?mcp";
scenarios["mcp-server-popup"].query = "?mcp";
scenarios["mcp-server-approval"].query = "?mcpApproval";
scenarios["mcp-server-settings-light"].query = "?mcp";

export default scenarios;
