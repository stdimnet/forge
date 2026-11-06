/** Code With Me relay scenarios: Settings | Tools | Code With Me and the connection paths. */

const perms = { files: "edit", terminal: "none", run: "none", vcs: "readOnly" };
const person = (id, name, role) => ({ id, name, role, color: id + 1, permissions: perms });

const hostSession = (relayed) => ({
  role: "host",
  canHost: true,
  me: 0,
  participants: [person(0, "Robert", "host"), person(1, "Alex", "guest"), person(2, "Sam", "guest")],
  files: [{ path: "app/Models/User.php" }],
  link: "forge://cwm/join?v=1&name=Robert&s=7ZQ2#h=abc",
  connections: [
    { participant: 1, kind: "direct", remote: "192.168.1.42:41641", rttMs: 3 },
    { participant: 2, kind: relayed ? "relay" : "direct", remote: relayed ? "https://relay.example.com/" : "10.8.0.7:52011", rttMs: relayed ? 48 : 21 },
  ],
  relay: { url: "https://relay.example.com/", connected: true },
});

/** @param {import("playwright-core").Page} page */
async function openRelaySettings(page) {
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForSelector(".set-dialog");
  await page.click('.set-tree-row[data-node="tools.code-with-me"]');
  await page.waitForSelector('[data-setting="codeWithMe.relayUrl"]');
  await page.fill('[data-setting="codeWithMe.relayUrl"] input', "relay.example.com");
  await page.click('[data-testid="cwm-relay-tools"] button:has-text("Test Connection")');
  await page.waitForSelector('[data-testid="cwm-relay-test"]');
  await page.locator('.set-section-title:has-text("Relay Server")').scrollIntoViewIfNeeded();
  await page.evaluate(() => {
    const title = [...document.querySelectorAll(".set-section-title")].find((t) => t.textContent?.includes("Relay Server"));
    title?.scrollIntoView({ block: "start" });
  });
}

/** @param {import("playwright-core").Page} page */
async function showSession(page) {
  await page.waitForFunction(() => !!window.__forgeCollabMock);
  await page.evaluate((v) => window.__forgeCollabMock.show(v), hostSession(true));
  await page.waitForSelector('[data-status="code-with-me"]');
  await page.click(".cwm-toolbar-btn");
  await page.waitForSelector(".cwm-popup .cwm-path");
}

const light = (page) => page.evaluate(() => (document.documentElement.dataset.theme = "light"));

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "cwm-relay-settings": openRelaySettings,
  "cwm-relay-settings-light": async (page) => {
    await light(page);
    await openRelaySettings(page);
  },
  "cwm-relay-token-dialog": async (page) => {
    await openRelaySettings(page);
    await page.click('[data-testid="cwm-relay-tools"] button:has-text("Set Token")');
    await page.fill("#cwm-relay-token", "team-secret-token");
    await page.click('.cwm-dialog button[title="Show"]');
  },
  "cwm-connection-paths": showSession,
  "cwm-connection-paths-light": async (page) => {
    await light(page);
    await showSession(page);
  },
};
