/**
 * Captures UI screenshots from the Vite dev server (browser mode with mocked backend).
 * Usage: pnpm dev &  then  pnpm screenshots [scenario...]
 * Scenarios are defined below; each opens the app, performs steps and writes
 * docs/screenshots/<name>.png. Set FORGE_URL / CHROMIUM_PATH to override defaults.
 */
import { chromium } from "playwright-core";
import { existsSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";

const url = process.env.FORGE_URL ?? "http://localhost:1420";
const outDir = resolve(import.meta.dirname, "../../../docs/screenshots");
const executablePath =
  process.env.CHROMIUM_PATH ??
  ["/opt/pw-browsers/chromium-1194/chrome-linux/chrome"].find((p) => existsSync(p));

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
const scenarios = {
  "workbench-dark": async () => {},
  "workbench-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  },
};

const wanted = process.argv.slice(2);
mkdirSync(outDir, { recursive: true });
const browser = await chromium.launch({ executablePath });
for (const [name, steps] of Object.entries(scenarios)) {
  if (wanted.length && !wanted.includes(name)) continue;
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  await page.goto(url);
  await page.waitForSelector(".cm-content");
  await steps(page);
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${outDir}/${name}.png` });
  console.log(`wrote ${name}.png`);
  await page.close();
}
await browser.close();
