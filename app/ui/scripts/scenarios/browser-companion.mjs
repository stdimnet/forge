/** Browser Companion: settings page, pairing, open requests, and the browser extension. */
import { chromium } from "playwright-core";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const EXTENSION_DIR = resolve(import.meta.dirname, "../../../../browser-extension");
const EXTENSION = join(EXTENSION_DIR, "dist/chrome");
const PROJECT_ROOT = "/home/me/Projects/acme-shop";
const chromiumPath =
  process.env.CHROMIUM_PATH ?? ["/opt/pw-browsers/chromium-1194/chrome-linux/chrome"].find((p) => existsSync(p));

/** @param {import("playwright-core").Page} page */
async function openCompanionSettings(page) {
  await page.evaluate(() => window.__forgeRunCommand("BrowserCompanion.Settings"));
  await page.waitForSelector('[data-testid="bc-server-status"]');
  await page.waitForSelector(".bc-card");
}

/** A stand-in for Forge's local API: answers ping (paired) and records open requests. */
async function startFakeForge() {
  const opened = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      const paired = req.headers.authorization === "Bearer screenshot-token";
      res.setHeader("Content-Type", "application/json");
      if (req.url === "/api/forge/ping") {
        const projects = paired ? { projects: [{ name: "acme-shop", root: PROJECT_ROOT }] } : {};
        res.end(JSON.stringify({ name: "Forge", version: "0.1.0", api: 1, paired, ...projects }));
      } else if (req.url === "/api/forge/open" && paired) {
        const request = JSON.parse(body);
        opened.push(request);
        res.end(JSON.stringify({ status: "opened", file: request.file, line: request.line }));
      } else {
        res.writeHead(404).end('{"error":"not found"}');
      }
    });
  });
  for (let port = 63342; port < 63352; port++) {
    const ok = await new Promise((done) => {
      server.once("error", () => done(false));
      server.listen(port, "127.0.0.1", () => done(true));
    });
    if (ok) return { server, opened, port };
  }
  throw new Error("no free port for the fake Forge API");
}

/** Hover `selector` with the real mouse and wait for the overlay to follow. */
async function hover(page, selector) {
  const box = await page.locator(selector).first().boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 5 });
  await page.waitForTimeout(250);
}

/**
 * Loads the real extension (dist/chrome) into Chromium, uses it on the demo shop
 * (browser-extension/demo) against a fake Forge API, and composes the page with the
 * toolbar popup into one browser-style image.
 * @param {import("playwright-core").Page} page
 */
async function captureExtension(page) {
  if (!existsSync(join(EXTENSION, "manifest.json"))) {
    execFileSync("node", [join(EXTENSION_DIR, "scripts/build.mjs")], { stdio: "inherit" });
  }
  const { serveDemo } = await import(join(EXTENSION_DIR, "scripts/demo-server.mjs"));
  const demo = await serveDemo(0);
  const demoUrl = `http://localhost:${demo.address().port}/`;
  const forge = await startFakeForge();
  const profile = mkdtempSync(join(tmpdir(), "forge-companion-"));
  const ctx = await chromium.launchPersistentContext(profile, {
    executablePath: chromiumPath,
    headless: true,
    args: [`--disable-extensions-except=${EXTENSION}`, `--load-extension=${EXTENSION}`],
    viewport: { width: 1440, height: 820 },
  });
  try {
    const sw = ctx.serviceWorkers()[0] ?? (await ctx.waitForEvent("serviceworker"));
    await sw.evaluate((port) => chrome.storage.local.set({ token: "screenshot-token", port }), forge.port);
    const shop = ctx.pages()[0] ?? (await ctx.newPage());
    await shop.goto(demoUrl);
    const tabId = await sw.evaluate(async () => (await chrome.tabs.query({ active: true }))[0].id);
    const inspect = (on) => sw.evaluate(([id, enabled]) => globalThis.forgeCompanion.setInspect(id, enabled), [tabId, on]);

    // Open a few components so the popup lists recent files.
    for (const selector of [".cart-btn", "#cart .total", ".card:nth-child(4) h3", ".card:nth-child(2) .add"]) {
      await inspect(true);
      await hover(shop, selector);
      await shop.mouse.down();
      await shop.mouse.up();
      await shop.waitForTimeout(250);
    }
    if (forge.opened.length !== 4) throw new Error(`expected 4 open requests, got ${JSON.stringify(forge.opened)}`);

    await inspect(true);
    await hover(shop, ".card:nth-child(2) h3");
    await shop.waitForSelector("forge-companion-overlay", { state: "attached" });
    const pageShot = await shop.screenshot();

    const popup = await ctx.newPage();
    await popup.setViewportSize({ width: 360, height: 600 });
    const popupTab = await sw.evaluate(async () => (await chrome.tabs.query({ active: true }))[0].id);
    await sw.evaluate((id) => globalThis.forgeCompanion.setInspect(id, true), popupTab);
    await popup.goto(sw.url().replace("background.js", "popup.html"));
    await popup.waitForSelector(".status.ok");
    await popup.waitForSelector("#recent li");
    const popupShot = await popup.locator("body").screenshot();
    const icon = readFileSync(join(EXTENSION, "icons/icon-32.png")).toString("base64");
    await page.setContent(composite(pageShot.toString("base64"), popupShot.toString("base64"), icon, demoUrl));
    await page.waitForFunction(() => [...document.images].every((i) => i.complete));
  } finally {
    await ctx.close();
    forge.server.close();
    demo.close();
    rmSync(profile, { recursive: true, force: true });
  }
}

/** A neutral browser frame around the page screenshot, with the popup under its icon. */
function composite(pagePng, popupPng, iconPng, url) {
  return `<!doctype html><html><head><style>
    body { margin: 0; width: 1440px; height: 900px; overflow: hidden; background: #dfe3e8; font: 13px Inter, system-ui, sans-serif; }
    .bar { height: 80px; box-sizing: border-box; background: #f1f3f5; border-bottom: 1px solid #cfd4da; }
    .tabs { height: 36px; display: flex; align-items: flex-end; padding-left: 12px; }
    .tab { height: 30px; width: 280px; white-space: nowrap; display: flex; align-items: center; gap: 8px; padding: 0 12px; background: #fff;
      border-radius: 8px 8px 0 0; color: #333; }
    .tab i { width: 14px; height: 14px; border-radius: 4px; background: linear-gradient(135deg, #2b59c3, #6a8de6); }
    .tools { height: 44px; display: flex; align-items: center; gap: 14px; padding: 0 14px; background: #fff; }
    .nav { display: flex; gap: 14px; color: #6b7280; font-size: 16px; }
    .url { flex: 1; height: 30px; display: flex; align-items: center; padding: 0 14px; border-radius: 15px; background: #f1f3f5; color: #374151; }
    .url b { font-weight: 400; color: #9ca3af; }
    .ext { position: relative; width: 30px; height: 30px; display: grid; place-items: center; border-radius: 50%; background: #e8eaed; }
    .ext img { width: 18px; height: 18px; }
    .badge { position: absolute; right: -6px; bottom: -2px; padding: 0 3px; border-radius: 3px; background: #ff7a1a; color: #1a0f06;
      font: 700 9px/12px Inter, system-ui, sans-serif; }
    .page { display: block; width: 1440px; height: 820px; }
    .popup { position: absolute; top: 74px; right: 10px; border-radius: 10px; box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
      border: 1px solid rgba(0, 0, 0, 0.25); }
  </style></head><body>
    <div class="bar">
      <div class="tabs"><div class="tab"><i></i>Acme Shop · Forge Companion demo</div></div>
      <div class="tools">
        <div class="nav"><span>&#8592;</span><span>&#8594;</span><span>&#8635;</span></div>
        <div class="url"><b>http://</b>${url.replace(/^http:\/\//, "")}</div>
        <div class="ext"><img src="data:image/png;base64,${iconPng}" alt=""><span class="badge">ON</span></div>
      </div>
    </div>
    <img class="page" src="data:image/png;base64,${pagePng}" alt="">
    <img class="popup" src="data:image/png;base64,${popupPng}" alt="">
  </body></html>`;
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "browser-companion-settings": async (page) => {
    await openCompanionSettings(page);
  },
  "browser-companion-pairing": async (page) => {
    await openCompanionSettings(page);
    await page.click('[data-testid="bc-pair"]');
    await page.waitForSelector('[data-testid="bc-pairing"]');
  },
  "browser-companion-integrations": async (page) => {
    await openCompanionSettings(page);
    await page.evaluate(() => {
      const title = [...document.querySelectorAll(".set-section-title")].find((t) => t.textContent === "Framework Integrations");
      title?.scrollIntoView({ block: "start" });
    });
  },
  "browser-companion-settings-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openCompanionSettings(page);
    await page.click('[data-testid="bc-pair"]');
    await page.waitForSelector('[data-testid="bc-pairing"]');
  },
  "browser-companion-opened": async (page) => {
    // As if the extension sent a click on <OrderController> to Forge.
    await page.evaluate(() => window.__forgeCompanionMock.open("app/Http/Controllers/OrderController.php", 12, 5));
    await page.waitForSelector(".cm-forge-flash");
    await page.waitForTimeout(150);
  },
  "browser-companion-confirm": async (page) => {
    await page.evaluate(() =>
      window.__forgeCompanionMock.open("/home/me/Downloads/legacy-lib/src/Legacy/Mailer.php", 40),
    );
    await page.waitForSelector(".bc-confirm[open]");
  },
  "browser-companion-extension": captureExtension,
};
